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
