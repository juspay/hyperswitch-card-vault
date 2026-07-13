CORES ?= 1
SCRIPT ?= production-traffic.js
# Defaults below are for a local HTTPS locker. Override them when running against docker.
BASE_URL ?= https://127.0.0.1:3001
TENANT_ID ?= public
DURATION ?= 5m
VUS ?= 5
FAILURE_LOG_FILE ?= ./results/unexpected-failures.jsonl
SUMMARY_FILE ?= ./results/summary.json

build:
	cd .. && docker build -t locker .

start:
	export CORES=$(CORES); \
		SCRIPT='$(SCRIPT)' \
		docker compose up -d
	echo "http://localhost:3000/d/k6/k6-load-testing-results?orgId=1&refresh=5s"

test:
	mkdir -p ./results
	: > $(FAILURE_LOG_FILE)
	export CORES=$(CORES); \
		SCRIPT='$(SCRIPT)' \
		docker compose run --rm k6 run \
			--console-output=/results/unexpected-failures.jsonl \
			/scripts/$(SCRIPT)
	@echo "Failure log: $(FAILURE_LOG_FILE)"
	@echo "Summary: $(SUMMARY_FILE)"

stop:
	export CORES=$(CORES); \
		SCRIPT='$(SCRIPT)' \
		docker compose down

local-test:
	mkdir -p ./results
	: > $(FAILURE_LOG_FILE)
	BASE_URL=$(BASE_URL) \
	TENANT_ID=$(TENANT_ID) \
	DURATION=$(DURATION) \
	VUS=$(VUS) \
	k6 run \
		--console-output=$(FAILURE_LOG_FILE) \
		./scripts/$(SCRIPT)
	@echo "Failure log: $(FAILURE_LOG_FILE)"
	@echo "Summary: $(SUMMARY_FILE)"
