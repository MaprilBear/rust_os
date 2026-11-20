*** Settings ***
Documentation     The ktest harness's own tests (tests-target/src/bin/ktest_selftest.rs).
...               Several fail on purpose; this suite checks that each outcome
...               is reported exactly as the protocol says.
Suite Setup       Setup
Suite Teardown    Teardown
Test Setup        Reset Emulation
Test Teardown     Test Teardown
Resource          ktest.resource

*** Test Cases ***
Harness Reports Every Outcome
    Load KTest Image    ktest_selftest    seed=7
    Start Emulation
    Wait For Log Entry    KTEST START suite=ktest_selftest tests=14 seed=7
    Wait For Log Entry    KTEST PASS ktest_selftest::passes
    # A failed assertion: file:line and the message, newlines escaped.
    Wait For Log Entry    KTEST FAIL ktest_selftest::assertion_fails tests-target/src/bin/ktest_selftest.rs:\\d+ assertion `left == right` failed: deliberate\\\\n\\s+left: 2\\\\n\\s+right: 3
    ...    treatAsRegex=true
    Wait For Log Entry    KTEST FAIL ktest_selftest::newline_in_message_is_escaped tests-target/src/bin/ktest_selftest.rs:\\d+ first\\\\nsecond$
    ...    treatAsRegex=true
    Wait For Log Entry    KTEST FAIL ktest_selftest::hangs - timed out after 50 ms
    # The next test runs on a fresh worker while the hung one still spins.
    Wait For Log Entry    KTEST PASS ktest_selftest::runs_after_a_hang
    # An MPU fault the test announced passes it.
    Wait For Log Entry    KTEST PASS ktest_selftest::kernel_ram_read_faults_as_expected
    # Reported where the test called expect_fault, not inside the harness.
    Wait For Log Entry    KTEST FAIL ktest_selftest::no_fault_when_one_was_expected tests-target/src/bin/ktest_selftest.rs:\\d+ expected a MemManage fault
    ...    treatAsRegex=true
    Wait For Log Entry    KTEST FAIL ktest_selftest::wrong_kind_of_fault - expected a MemManage fault with MMFSR bits 0x1, got MMFSR=0x
    Wait For Log Entry    KTEST PASS ktest_selftest::runs_after_failures_and_faults
    Wait For Log Entry    KTEST PASS ktest_selftest::rng_follows_the_seed
    # Late outcomes stay with their own test.
    Wait For Log Entry    KTEST FAIL ktest_selftest::faults_after_its_timeout - timed out after 30 ms
    Wait For Log Entry    KTEST FAIL ktest_selftest::hangs_after_a_late_fault - timed out after 100 ms
    Wait For Log Entry    KTEST FAIL ktest_selftest::panics_after_its_timeout - timed out after 30 ms
    Wait For Log Entry    KTEST PASS ktest_selftest::passes_after_a_late_panic
    Wait For Log Entry    KTEST DONE pass=6 fail=8
    Should Not Be In Log    late panic    timeout=1
    # DONE is out, so a short look suffices (the default waits the full timeout).
    Should Not Be In Log    KTEST ABORT    timeout=1

Seed Defaults To 1 Without The Magic
    Execute Command    mach create "nrf52840"
    Execute Command    machine LoadPlatformDescription @${CURDIR}/../nrf52840.repl
    Execute Command    sysbus LoadELF @${KTEST_DIR}/ktest_selftest
    Create Log Tester    ${KTEST_TIMEOUT}
    Start Emulation
    Wait For Log Entry    KTEST START suite=ktest_selftest tests=14 seed=1
