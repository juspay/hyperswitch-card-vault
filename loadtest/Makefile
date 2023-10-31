CORES = 1

build:
	cd .. && docker build -t locker .

start:
	export CORES=$(CORES); \
		SCRIPT='http-rs.js' \
		docker compose up -d
	echo "http://localhost:3000/d/k6/k6-load-testing-results?orgId=1&refresh=5s"

test:
	export CORES=$(CORES); \
		SCRIPT='http-rs.js' \
		docker compose up k6

stop:
	export CORES=$(CORES); \
		SCRIPT='http-rs.js' \
		docker compose down
