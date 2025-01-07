.PHONY: watchdog

watchdog:
	cp settings.example.yml ./local_dev/settings.yml
	sed -i "s|PGBOUNCER_LOG|$(shell pwd)/local_dev/pgbouncer/pgbouncer.log|g" local_dev/settings.yml
	sed -i "s|PGBOUNCER_OUT|$(shell pwd)/local_dev/pgbouncer.out|g" local_dev/settings.yml
	cargo build --release
	./target/release/log-watchdog --settings ./local_dev/settings.yml

test:
	cargo test --all