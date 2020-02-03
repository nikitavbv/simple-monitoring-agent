use std::env;

use chrono::{DateTime, Utc};
use futures::future::{try_join_all, try_join};
use futures::{TryFutureExt, TryStreamExt};
use custom_error::custom_error;

use crate::database::Database;

#[derive(Debug, Clone)]
pub struct PostgresStat {
    timestamp: DateTime<Utc>,
    database_stat: DatabaseStat,
    table_stat: Vec<TableStat>
}

#[derive(Debug, Clone)]
pub struct DatabaseStat {
    tup_returned: i32,
    tup_fetched: i32,
    tup_inserted: i32,
    tup_updated: i32,
    tup_deleted: i32
}

#[derive(Debug, Clone)]
pub struct TableStat {
    table: String,
    rows: f32,
    total_bytes: i64
}

#[derive(Debug, Clone)]
pub struct PostgresMetric {
    timestamp: DateTime<Utc>,
    database_metric: DatabaseMetric,
    table_metrics: Vec<TableMetric>
}

#[derive(Debug, Clone)]
pub struct DatabaseMetric {
    tup_returned: i32,
    tup_fetched: i32,
    tup_inserted: i32,
    tup_updated: i32,
    tup_deleted: i32
}

#[derive(Debug, Clone)]
pub struct TableMetric {
    table: String,
    rows: i32,
    total_bytes: i64
}

custom_error!{pub PostgresMetricError
    NotConfigured = "database to monitor not set",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed"
}

fn get_postgres_database_name() -> Option<String> {
    env::var("DATABASE_TO_MONITOR").ok()
}

pub async fn monitor_postgres(mut database: &Database) -> Result<PostgresStat, PostgresMetricError> {
    let database_to_monitor = match get_postgres_database_name() {
        Some(v) => v,
        None => return Err(PostgresMetricError::NotConfigured)
    };

    let timestamp = Utc::now();

    let database_stat = sqlx::query!(
        "select cast(tup_returned as int), cast(tup_fetched as int), cast(tup_inserted as int), cast(tup_updated as int), cast(tup_deleted as int) from pg_stat_database where datname = cast($1 as text) limit 1",
        database_to_monitor
    ).fetch_one(&mut database).map_ok(|rec| DatabaseStat {
        tup_returned: rec.tup_returned,
        tup_fetched: rec.tup_fetched,
        tup_inserted: rec.tup_inserted,
        tup_updated: rec.tup_updated,
        tup_deleted: rec.tup_deleted
    }).await?;

    let table_stat = sqlx::query!(r"
SELECT cast(table_name as text), row_estimate, total_bytes AS total
  FROM (
  SELECT *, total_bytes-index_bytes-COALESCE(toast_bytes,0) AS table_bytes FROM (
      SELECT c.oid,nspname AS table_schema, relname AS TABLE_NAME
              , c.reltuples AS row_estimate
              , pg_total_relation_size(c.oid) AS total_bytes
              , pg_indexes_size(c.oid) AS index_bytes
              , pg_total_relation_size(reltoastrelid) AS toast_bytes
          FROM pg_class c
          LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE relkind = 'r' and relname not like 'pg_%' and relname not like 'sql_%'
  ) a
) a;").fetch(&mut database).map_ok(|rec| TableStat {
        table: rec.table_name,
        rows: rec.row_estimate,
        total_bytes: rec.total,
    }).try_collect().await?;

    Ok(PostgresStat {
        timestamp,
        database_stat,
        table_stat
    })
}

pub fn postgres_metric_from_stats(first: &PostgresStat, second: &PostgresStat) -> PostgresMetric {
    PostgresMetric {
        timestamp: second.timestamp,
        table_metrics: second.table_stat.iter().map(|v| TableMetric {
            table: v.table.clone(),
            rows: v.rows as i32,
            total_bytes: v.total_bytes,
        }).collect(),
        database_metric: table_metric_from_two_stats(&first.database_stat, &second.database_stat)
    }
}

fn table_metric_from_two_stats(first: &DatabaseStat, second: &DatabaseStat) -> DatabaseMetric {
    DatabaseMetric {
        tup_returned: second.tup_returned - first.tup_returned,
        tup_fetched: second.tup_fetched - first.tup_fetched,
        tup_inserted: second.tup_inserted - first.tup_inserted,
        tup_updated: second.tup_updated - first.tup_updated,
        tup_deleted: second.tup_deleted - first.tup_deleted
    }
}

pub async fn save_postgres_metric(database: &Database, hostname: &str, metric: &PostgresMetric) -> Result<(), PostgresMetricError> {
    let metric = metric.clone();

    let timestamp = metric.timestamp.clone();

    let futures = metric.table_metrics.into_iter()
        .map(|entry| save_table_metric_entry(&database, hostname, &timestamp, entry));

    try_join(
        try_join_all(futures),
        save_database_metric(&database, hostname, &timestamp, metric.database_metric)
    ).await?;

    Ok(())
}

async fn save_table_metric_entry(mut database: &Database, hostname: &str, timestamp: &DateTime<Utc>, entry: TableMetric) -> Result<(), PostgresMetricError> {
    sqlx::query!(
        "insert into metric_postgres_tables (hostname, timestamp, name, rows, total_bytes) values ($1, $2, $3, $4, $5) returning hostname",
        hostname.to_string(), *timestamp, entry.table, entry.rows, entry.total_bytes
    ).fetch_one(&mut database).await?;

    Ok(())
}

async fn save_database_metric(mut database: &Database, hostname: &str, timestamp: &DateTime<Utc>, entry: DatabaseMetric) -> Result<(), PostgresMetricError> {
    sqlx::query!(
        "insert into metric_postgres_database (hostname, timestamp, returned, fetched, inserted, updated, deleted) values ($1, $2, $3, $4, $5, $6, $7)",
        hostname.to_string(), *timestamp, entry.tup_returned, entry.tup_fetched, entry.tup_inserted, entry.tup_updated, entry.tup_deleted
    ).fetch_one(&mut database).await?;

    Ok(())
}