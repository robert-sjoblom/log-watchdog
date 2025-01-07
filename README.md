# What is?

Watch thing, do thing.

log-watchdog will keep an eye on your log files for you. if something happens that it should react to, it will do so and then resume its vigil (unless it shouldn't, in which case it won't).

# Configuration

The configuration is written in yaml:

```yaml
watchdogs:
  pgbouncer:
    log_file: /var/log/pgbouncer/pgbouncer.log
    output_file: /opt/wathdog/pgbouncer.out
    debounce: 5000
    oneshot: false
    regex: .*
    commands:
      echo:
        args:
          - "hello world!"
          - -v
```

log-watchdog can watch several different logs, or run several commands on a match on one log.

# Usage

```bash
./log-watchdog --settings path/to/settings/file.yml
```

## Pgbouncer

If we want to watch pgbouncer log, we'll use local dev docker-compose setup.

1. create folder pgbouncer in local dev
2. chmod -R 757 ./pgbouncer
