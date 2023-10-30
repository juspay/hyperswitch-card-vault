

docker-run:
	docker run -v `pwd`/config/docker-configuration.toml:/local/config/development.toml -p 8080:8080 -d locker

docker-it-run:
	docker run -v `pwd`/config/docker-configuration.toml:/local/config/development.toml -it locker /bin/bash
