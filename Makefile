IMAGE      ?= nrf52840-os-dev
TARGET     := thumbv7em-none-eabihf
PROFILE    ?= debug
BIN        := kernel
ELF        := target/$(TARGET)/$(PROFILE)/$(BIN)
GDB_PORT   ?= 3334

# HOME must be writable: Renode keeps its config and temp files there.
DOCKER_RUN := docker run --rm -it \
	--user $(shell id -u):$(shell id -g) \
	-e HOME=/tmp \
	-v $(CURDIR):/work \
	-v nrf52840-cargo-home:/cargo-home \
	-w /work

.PHONY: image build release run debug gdb test qemu qemu-debug shell clippy fmt size clean

image:
	docker build -t $(IMAGE) .

build:
	$(DOCKER_RUN) $(IMAGE) cargo build

release:
	$(DOCKER_RUN) $(IMAGE) cargo build --release

# Boot the kernel on a simulated nRF52840 in Renode. Type `quit` to exit.
run: build
	$(DOCKER_RUN) $(IMAGE) renode --disable-gui --console renode/run.resc

# Renode with a GDB server on :$(GDB_PORT); CPU starts when GDB connects.
# Host networking keeps the GDB port reachable at localhost.
debug: build
	$(DOCKER_RUN) --network host $(IMAGE) renode --disable-gui --console renode/debug.resc

# Second terminal, while `make debug` is running.
gdb:
	$(DOCKER_RUN) --network host $(IMAGE) gdb-multiarch $(ELF) \
		-ex "target remote localhost:$(GDB_PORT)"

# Robot Framework tests (renode/*.robot).
test: build
	$(DOCKER_RUN) $(IMAGE) renode-test renode/test.robot

# Optional: core-only check on QEMU's generic Cortex-M4 board (no nRF peripherals).
qemu: build
	$(DOCKER_RUN) $(IMAGE) qemu-system-arm -machine mps2-an386 -cpu cortex-m4 \
		-nographic -semihosting-config enable=on,target=native -kernel $(ELF)

qemu-debug: build
	$(DOCKER_RUN) --network host $(IMAGE) qemu-system-arm -machine mps2-an386 \
		-cpu cortex-m4 -nographic -semihosting-config enable=on,target=native \
		-kernel $(ELF) -S -gdb tcp::1234

shell:
	$(DOCKER_RUN) $(IMAGE) bash

clippy:
	$(DOCKER_RUN) $(IMAGE) cargo clippy

fmt:
	$(DOCKER_RUN) $(IMAGE) cargo fmt

size:
	$(DOCKER_RUN) $(IMAGE) cargo size -- -A

clean:
	$(DOCKER_RUN) $(IMAGE) cargo clean
	rm -f log.html report.html robot_output.xml