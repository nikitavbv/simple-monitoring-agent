# Simple monitoring agent

This is a lightweight tool which inside docker container or as a service directly on your server and reports key 
metrics (see list below) to Postgres.

No UI is included, so you will probably need to use Grafana with Postgres connector.

There are a lot of other better tools for monitoring out there. Still, this works fine for my personal projects
and I had some fun writing it :)

Supported metrics:
 - cpu load
 - load average
 - memory usage (ram, swap)
 - io
 - filesystem usage
 - network io
 - nginx: handled requests
 - postgres: database operations stats, disk usage, total rows.
 - docker: running container status and stats (cpu, memory, io usage)