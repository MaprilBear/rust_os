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
    # Ten whole lines, five of each. Renode sends a DMA buffer whole, so
    # without the TX lock lines are not mixed but lost (a sender whose
    # ENDTX wake-up went to the other is never woken): a line goes missing.
    ${a}=    Set Variable    ${0}
    ${b}=    Set Variable    ${0}
    FOR    ${i}    IN RANGE    10
        ${line}=    Wait For Next Line On Uart    testerId=${uart}
        Should Match Regexp    ${line.Line}    ^(${LINE_A}|${LINE_B})$
        IF    '${line.Line}' == '${LINE_A}'
            ${a}=    Evaluate    ${a} + 1
        ELSE
            ${b}=    Evaluate    ${b} + 1
        END
    END
    Should Be Equal As Integers    ${a}    5
    Should Be Equal As Integers    ${b}    5
    Wait For Log Entry    KTEST RUN driver_uarte::receive_gets_the_typed_bytes
    Write To Uart    ktest    testerId=${uart}
    Wait For Log Entry    KTEST RUN driver_uarte::receive_keeps_the_rest_queued
    Write To Uart    0123456789    testerId=${uart}
    ${line}=    Wait For Log Entry    KTEST (DONE|ABORT)    treatAsRegex=true
    Should Contain    ${line}    KTEST DONE
    Should Contain    ${line}    fail=0

TIMER
    Run KTest Suite    driver_timer

PPI
    Run KTest Suite    driver_ppi

Driver Known Issues
    [Documentation]    Known issue 3: timers share one semaphore.
    [Tags]    known-issue
    Run KTest Suite    driver_known_issues
