*** Settings ***
Documentation     Runs one ktest suite image (tests-target) and checks its
...               KTEST protocol lines on the debug console. See
...               docs/TESTING.md, tier 2.
Resource          ${RENODEKEYWORDS}

*** Variables ***
# Set by `cargo xtask test-renode`: the directory holding the suite images.
${KTEST_DIR}      ${CURDIR}/../../target/thumbv7em-none-eabihf/debug
${SEED}           1
# CPU speed for the suites; `default` keeps Renode's. Changing it moves
# interrupts and ticks relative to the code (e.g. --variable MIPS:5).
${MIPS}           default
# Virtual seconds a suite may take before the runner gives up.
${KTEST_TIMEOUT}  120

*** Keywords ***
Load KTest Image
    [Arguments]    ${image}    ${seed}=${SEED}
    Execute Command    mach create "nrf52840"
    Execute Command    machine LoadPlatformDescription @${CURDIR}/../nrf52840.repl
    Execute Command    sysbus LoadELF @${KTEST_DIR}/${image}
    # The seed cell is in .uninit (link.ld), which boot leaves alone.
    ${addr}=    Execute Command    sysbus GetSymbolAddress "KTEST_SEED"
    ${addr}=    Evaluate    int("""${addr}""".strip(), 16)
    Execute Command    sysbus WriteDoubleWord ${addr} 0x5EED5EED
    Execute Command    sysbus WriteDoubleWord ${{${addr} + 4}} ${seed}
    IF    '${MIPS}' != 'default'
        Execute Command    cpu PerformanceInMips ${MIPS}
    END
    Create Log Tester    ${KTEST_TIMEOUT}

Run KTest Suite
    [Documentation]    Boot ${image} and require "KTEST DONE ... fail=0".
    ...                Any FAIL or ABORT line fails the test; the full
    ...                emulator log (with every KTEST line) is in the report.
    [Arguments]    ${image}    ${seed}=${SEED}
    Load KTest Image    ${image}    ${seed}
    Start Emulation
    ${line}=    Wait For Log Entry    KTEST (DONE|ABORT)    treatAsRegex=true
    Should Contain    ${line}    KTEST DONE
    Should Contain    ${line}    fail=0
