#!/sh
# Cargo runner: `cargo run` passes the built ELF as $1.
BINARY_PATH="$1"
shift # Remove the binary path from the arguments

# Default Renode script
RESC_FILE="renode/run.resc"

# Scan through arguments to look for the --debug flag
while [ $# -gt 0 ]; do
    if [ "$1" = "--debug" ]; then
        RESC_FILE="renode/debug.resc"
    fi
    shift
done

# Execute Renode with the chosen script
exec renode --disable-gui --console \
    -e "\$bin=@$BINARY_PATH" \
    -e "include @$RESC_FILE"