VERSION := $(shell cat VERSION)
IMAGE := harbor.jinqidongli.com/x9-rust/yrs-server

run:
	RUST_LOG=info cargo run

debug:
	RUST_LOG=debug cargo run

build:
	cargo build

release:
	cross build --target x86_64-unknown-linux-gnu --release

clean:
	cargo clean

rebuild: clean build

docker-build:
	docker build -t $(IMAGE):prod-$(VERSION) .
	@awk -F. '{ $$3++ } 1' OFS=. VERSION > VERSION.tmp && mv VERSION.tmp VERSION
	@echo "use image \"$(IMAGE):$(VERSION)\""

docker-push: docker-build
	docker push $(IMAGE):$(VERSION)
