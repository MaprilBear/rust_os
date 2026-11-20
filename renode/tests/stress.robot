*** Settings ***
Documentation     On-target stress (tests-target/src/bin/stress_kernel.rs). One run
...               per seed and CPU speed: `cargo xtask test-renode` runs seed 1 at
...               the default speed; `cargo xtask stress` sweeps
...               (--variable SEEDS:1,2,3 --variable MIPS_LIST:default,10,50).
...               Virtual time is deterministic: a failing seed/speed pair reproduces.
Suite Setup       Setup
Suite Teardown    Teardown
Test Teardown     Test Teardown
Library           String
Resource          ktest.resource

*** Variables ***
${SEEDS}          1
${MIPS_LIST}      default

*** Test Cases ***
Stress Sweep
    @{seeds}=    Split String    ${SEEDS}    ,
    @{speeds}=    Split String    ${MIPS_LIST}    ,
    FOR    ${mips}    IN    @{speeds}
        FOR    ${seed}    IN    @{seeds}
            Run Keyword And Continue On Failure    Stress Once    ${seed}    ${mips}
        END
    END

*** Keywords ***
Stress Once
    [Arguments]    ${seed}    ${mips}
    Reset Emulation
    # ktest.resource sets the CPU speed from ${MIPS}.
    Set Test Variable    ${MIPS}    ${mips}
    Load KTest Image    stress_kernel    ${seed}
    Start Emulation
    ${line}=    Wait For Log Entry    KTEST (DONE|ABORT)    treatAsRegex=true
    Should Contain    ${line}    fail=0    msg=seed ${seed} at ${mips} MIPS: ${line}
