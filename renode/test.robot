*** Settings ***
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          ${RENODEKEYWORDS}

*** Variables ***
${ELF}            ${CURDIR}/../target/thumbv7em-none-eabihf/debug/kernel

*** Keywords ***
Create Machine
    Execute Command    mach create "nrf52840"
    Execute Command    machine LoadPlatformDescription @${CURDIR}/nrf52840.repl
    Execute Command    sysbus LoadELF @${ELF}
    # The kernel prints through the debug console peripheral (src/debugcon.rs),
    # which writes to Renode's log, so watch the log rather than a UART.
    Create Log Tester    5

*** Test Cases ***
Kernel Prints Banner On Debug Console
    [Documentation]    Passes once the kernel prints "kernel booted" with dprintln!.
    Create Machine
    Start Emulation
    Wait For Log Entry    debugcon: kernel booted
