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

