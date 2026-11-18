IMAGE      ?= nrf52840-os-dev
TARGET     := thumbv7em-none-eabihf
PROFILE    ?= debug
BIN        := rust_os
ELF        := target/$(TARGET)/$(PROFILE)/$(BIN)
MAP        := $(ELF).map
GDB_PORT   ?= 3333

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

# Every build is followed by the kernel/user placement check (tools/check_placement.py):
# it fails the build if the kernel crate did not land in KFLASH/KRAM (e.g. LTO got
# turned on) or if anything else landed there.
build:
	$(DOCKER_RUN) $(IMAGE) sh -c 'cargo build && python3 tools/check_placement.py $(MAP)'

release:
	$(DOCKER_RUN) $(IMAGE) sh -c 'cargo build --release && python3 tools/check_placement.py target/$(TARGET)/release/$(BIN).map'

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
	$(DOCKER_RUN) $(IMAGE) cargo clippy --workspace

fmt:
	$(DOCKER_RUN) $(IMAGE) cargo fmt --all

size:
	$(DOCKER_RUN) $(IMAGE) cargo size --bin $(BIN) -- -A

clean:
	$(DOCKER_RUN) $(IMAGE) cargo clean
	rm -f log.html report.html robot_output.xml