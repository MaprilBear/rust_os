*** Settings ***
Documentation     Kernel suites on the real kernel in Renode (tests-target/src/bin/kernel_*.rs).
...               Each test boots one image and requires "KTEST DONE ... fail=0";
...               the report's log has every KTEST line.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          ktest.resource

*** Test Cases ***
Scheduling
    Run KTest Suite    kernel_sched

Semaphores
    Run KTest Suite    kernel_sem

Hostile Syscalls
    Run KTest Suite    kernel_syscalls

MPU Attacks
    Run KTest Suite    kernel_mpu

Floating Point In Threads
    [Documentation]    Known issue 18: the FPU is never enabled.
    [Tags]    known-issue
    Run KTest Suite    kernel_fpu
