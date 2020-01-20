use std::env;

use sqlx::{PgPool, Pool, Error as SQLXError, PgConnection};

pub type Database = Pool<PgConnection>;

fn get_connection_string() -> String {
    env::var("DATABASE_URL").expect("DATABASE_URL envvar is not set")
}

pub async fn connect() -> Result<Database, SQLXError> {
    PgPool::new(&get_connection_string()).await
}