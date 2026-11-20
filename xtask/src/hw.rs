//! `cargo xtask test-hw`: the on-target suites (tests-target) on an
//! nRF52840-DK, through probe-rs. Each image is flashed and run with
//! `probe-rs run`, which streams the image's RTT output (the KTEST lines,
//! docs/TESTING.md) to stdout; the runner reads until `KTEST DONE` or
//! `KTEST ABORT`, or gives up after a timeout.
//!
//! Which images run, and what each must report, is [`IMAGES`]: the
//! equivalent of the Robot files' expectations and tags.

/// What an image must report.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Expect {
    /// `KTEST DONE` with fail=0.
    AllPass,
    /// Exactly this `KTEST DONE` line's counts, e.g. "pass=6 fail=8".
    Done(&'static str),
    /// Not run on hardware, and why.
    Skip(&'static str),
}

/// Every image in tests-target/src/bin, and what it must report on the DK.
/// An image missing here runs with [`Expect::AllPass`].
pub const IMAGES: &[(&str, Expect)] = &[
    ("ktest_selftest", Expect::Done("pass=6 fail=8")),
    ("kernel_sched", Expect::AllPass),
    ("kernel_sem", Expect::AllPass),
    ("kernel_syscalls", Expect::AllPass),
    ("kernel_mpu", Expect::AllPass),
    ("kernel_fpu", Expect::Skip("known issue 18 (aborts)")),
    ("driver_timer", Expect::AllPass),
    ("driver_ppi", Expect::AllPass),
    (
        "driver_known_issues",
        Expect::Skip("known issue 3 (deadlocks by design)"),
    ),
    (
        "driver_uarte",
        Expect::Skip("needs the UART's other end: Robot drives it in Renode"),
    ),
    ("stress_kernel", Expect::AllPass),
];

pub fn expectation(image: &str) -> Expect {
    IMAGES
        .iter()
        .find(|(name, _)| *name == image)
        .map_or(Expect::AllPass, |(_, e)| *e)
}

/// One KTEST line, from wherever it sits in a line of probe-rs output.
#[derive(Debug, PartialEq)]
pub enum Event<'a> {
    Start,
    Run,
    Pass,
    Fail(&'a str),
    Abort(&'a str),
    Done {
        pass: u32,
        fail: u32,
    },
    /// INFO and anything else the harness prints.
    Other,
}

/// The KTEST line in `line`, if any.
pub fn parse(line: &str) -> Option<Event<'_>> {
    let rest = &line[line.find("KTEST ")? + "KTEST ".len()..];
    let (word, tail) = rest.split_once(' ').unwrap_or((rest, ""));
    Some(match word {
        "START" => Event::Start,
        "RUN" => Event::Run,
        "PASS" => Event::Pass,
        "FAIL" => Event::Fail(tail),
        "ABORT" => Event::Abort(tail),
        "DONE" => {
            let count = |key: &str| {
                tail.split(' ')
                    .find_map(|kv| kv.strip_prefix(key)?.parse().ok())
            };
            match (count("pass="), count("fail=")) {
                (Some(pass), Some(fail)) => Event::Done { pass, fail },
                _ => Event::Other,
            }
        }
        _ => Event::Other,
    })
}

/// How an image's run ended.
#[derive(Debug, PartialEq)]
pub enum End {
    Done {
        pass: u32,
        fail: u32,
    },
    Abort(String),
    /// No DONE or ABORT before the timeout.
    Silent,
    /// probe-rs exited before DONE or ABORT (e.g. it lost the probe).
    ProbeExited,
}

/// Whether `end` is what `expect` asks for.
pub fn verdict(expect: Expect, end: &End) -> Result<(), String> {
    match (expect, end) {
        (Expect::Skip(_), _) => Ok(()),
        (Expect::AllPass, End::Done { fail: 0, .. }) => Ok(()),
        (Expect::Done(want), End::Done { pass, fail })
            if want == format!("pass={pass} fail={fail}") =>
        {
            Ok(())
        }
        (Expect::AllPass, End::Done { pass, fail }) => {
            Err(format!("{fail} failed ({pass} passed)"))
        }
        (Expect::Done(want), End::Done { pass, fail }) => {
            Err(format!("expected {want}, got pass={pass} fail={fail}"))
        }
        (_, End::Abort(why)) => Err(format!("aborted: {why}")),
        (_, End::Silent) => Err("no KTEST DONE before the timeout".into()),
        (_, End::ProbeExited) => Err("probe-rs exited before KTEST DONE".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_ktest_lines_inside_probe_output() {
        assert_eq!(parse("KTEST PASS kernel_sem::x"), Some(Event::Pass));
        assert_eq!(parse("00:00.012 KTEST RUN kernel_sem::x"), Some(Event::Run));
        assert_eq!(parse("rtt: some other output"), None);
        assert_eq!(parse("KTEST INFO stress seed=1"), Some(Event::Other));
    }

    #[test]
    fn reads_fail_abort_and_done() {
        assert_eq!(
            parse("KTEST FAIL s::t a.rs:3 boom"),
            Some(Event::Fail("s::t a.rs:3 boom"))
        );
        assert_eq!(
            parse("KTEST ABORT MemManage: CFSR=0x82"),
            Some(Event::Abort("MemManage: CFSR=0x82"))
        );
        assert_eq!(
            parse("KTEST DONE pass=6 fail=8"),
            Some(Event::Done { pass: 6, fail: 8 })
        );
        assert_eq!(parse("KTEST DONE pass=x"), Some(Event::Other), "malformed");
    }

    #[test]
    fn verdicts() {
        let done = |pass, fail| End::Done { pass, fail };
        assert!(verdict(Expect::AllPass, &done(5, 0)).is_ok());
        assert!(verdict(Expect::AllPass, &done(4, 1)).is_err());
        assert!(verdict(Expect::Done("pass=6 fail=8"), &done(6, 8)).is_ok());
        assert!(verdict(Expect::Done("pass=6 fail=8"), &done(7, 7)).is_err());
        assert!(verdict(Expect::AllPass, &End::Abort("x".into())).is_err());
        assert!(verdict(Expect::AllPass, &End::Silent).is_err());
        assert!(verdict(Expect::AllPass, &End::ProbeExited).is_err());
        assert!(verdict(Expect::Skip("why"), &End::Silent).is_ok());
    }

    #[test]
    fn every_listed_image_exists() {
        let bins = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests-target/src/bin");
        for (image, _) in IMAGES {
            assert!(bins.join(format!("{image}.rs")).exists(), "{image}");
        }
    }

    #[test]
    fn every_image_is_listed() {
        let bins = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests-target/src/bin");
        for entry in std::fs::read_dir(bins).unwrap() {
            let path = entry.unwrap().path();
            let image = path.file_stem().unwrap().to_str().unwrap().to_string();
            assert!(
                IMAGES.iter().any(|(name, _)| *name == image),
                "{image} is not in hw::IMAGES"
            );
        }
    }
}
