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
    Create Terminal Tester    sysbus.uart0

*** Test Cases ***
Kernel Prints Banner On UART0
    [Documentation]    Fails until the kernel writes "kernel booted" to UARTE0.
    Create Machine
    Start Emulation
    Wait For Line On Uart    kernel booted    timeout=5
