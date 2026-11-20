*** Settings ***
Documentation     Driver suites on the simulated peripherals (tests-target/src/bin/driver_*.rs).
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          ktest.resource

*** Variables ***
${LINE_A}         <AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA>
${LINE_B}         <BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB>

*** Test Cases ***
UARTE
    Load KTest Image    driver_uarte
    ${uart}=    Create Terminal Tester    sysbus.uart0    defaultPauseEmulation=false
    Start Emulation
    Wait For Line On Uart    uarte: hello    testerId=${uart}
    Wait For Line On Uart    uarte: hello    testerId=${uart}
    # Ten whole lines, five of each, in any order: a line mixing A and B
    # would match neither.
    FOR    ${i}    IN RANGE    10
        ${line}=    Wait For Next Line On Uart    testerId=${uart}
        Should Match Regexp    ${line.Line}    ^(${LINE_A}|${LINE_B})$
    END
    Wait For Log Entry    KTEST RUN driver_uarte::receive_gets_the_typed_bytes
    Write To Uart    ktest    testerId=${uart}
    Wait For Log Entry    KTEST RUN driver_uarte::receive_returns_at_most_the_buffer
    Write To Uart    0123456789    testerId=${uart}
    ${line}=    Wait For Log Entry    KTEST (DONE|ABORT)    treatAsRegex=true
    Should Contain    ${line}    fail=0

TIMER
    Run KTest Suite    driver_timer

PPI
    [Documentation]    Renode's PPI model does not route these events.
    [Tags]    dk-only
    Run KTest Suite    driver_ppi

Driver Known Issues
    [Documentation]    Known issue 3: timers share one semaphore.
    [Tags]    known-issue
    Run KTest Suite    driver_known_issues
