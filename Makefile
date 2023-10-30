

docker-run:
	docker run -v `pwd`/config/docker-configuration.toml:/local/config/development.toml locker -d

docker-it-run:
	docker run -v `pwd`/config/docker-configuration.toml:/local/config/development.toml -it locker /bin/bash
