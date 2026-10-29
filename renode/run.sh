#!/bin/sh
# Cargo runner: `cargo run` passes the built ELF as $1.
exec renode --disable-gui --console \
    -e "\$bin=@$1" \
    -e "include @renode/run.resc"