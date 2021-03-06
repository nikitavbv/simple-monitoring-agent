create table metric_cpu
(
    hostname text,
    cpu integer not null,
    "user" integer,
    nice integer,
    system integer,
    idle integer,
    iowait integer,
    irq integer,
    softirq integer,
    guest integer,
    steal integer,
    guest_nice integer,
    timestamp timestamp with time zone
);

create table metric_load_average
(
	hostname text not null,
	timestamp timestamp with time zone not null,
	one double precision not null,
	five double precision not null,
	fifteen double precision not null
);

create table metric_memory
(
    hostname text not null,
    timestamp timestamp with time zone,
    total bigint,
    free bigint,
    available bigint,
    buffers bigint,
    cached bigint,
    swap_total bigint,
    swap_free bigint
);

create table metric_io
(
    hostname text not null,
    timestamp timestamp with time zone not null,
    device text not null,
    read double precision not null,
    write double precision not null
);

create table metric_fs
(
    hostname text,
    timestamp timestamp with time zone,
    filesystem text,
    total bigint,
    used bigint
);

create table metric_network
(
    hostname text,
    timestamp timestamp with time zone,
    device text,
    rx double precision,
    tx double precision
);

create table metric_docker_containers
(
    hostname text not null,
    timestamp timestamp with time zone not null,
    name text,
    state text not null,
    cpu_usage double precision not null,
    memory_usage bigint not null,
    memory_cache bigint not null,
    network_tx double precision not null,
    network_rx double precision not null
);

create table metric_nginx
(
    hostname text not null,
    timestamp timestamp with time zone not null,
    handled_requests integer not null
);

create table metric_postgres_database
(
    hostname text,
    timestamp timestamp with time zone,
    returned integer,
    fetched integer,
    inserted integer,
    updated integer,
    deleted integer
);

create table metric_postgres_tables
(
    hostname text not null,
    timestamp timestamp with time zone not null,
    name text not null,
    rows integer not null,
    total_bytes bigint not null
);
