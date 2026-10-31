# Renode Python peripheral: a one-register "debug console".
#
# Registered in renode/nrf52840.repl at 0x60000000. Any 32-bit write to it
# appends the low byte to a line buffer; on '\n' the line is printed to the
# Renode log as `debugcon: <line>`. This does not exist on real hardware, so it
# is only for debugging in the simulator (see src/debugcon.rs).
#
# `request` and `self` are provided by Renode. Variables set during IsInit
# persist between calls.

if request.IsInit:
    line = ""
elif request.IsWrite:
    c = request.Value & 0xFF
    if c == 10:
        # "{0}" + argument, so braces in your text can't break log formatting.
        self.InfoLog("{0}", line)
        line = ""
    elif c != 13:
        line += chr(c)
elif request.IsRead:
    request.Value = 0