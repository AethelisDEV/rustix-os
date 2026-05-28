//! # Triple Modular Redundancy (TMR) Voting Module
//!
//! This module implements a Triple Modular Redundancy (TMR) framework for the AE Rustanium kernel.
//!
//! In safety-critical computing environments (such as aerospace flight control or medical life support),
//! single event upsets (SEUs) can transiently flip registers, program counters, or ALU data lines.
//! To mitigate this without relying solely on specialized radiation-hardened hardware, TMR is employed.
//!
//! ## How TMR Works
//! 1. A critical task is executed in three independent virtual contexts (Runners 0, 1, and 2).
//! 2. The outputs of all three runners are fed into a secure majority voter.
//! 3. If all three outputs are identical, execution is perfect.
//! 4. If one runner produces a divergent output due to a simulated register bit flip:
//!    - The voter detects the exact divergent runner (0, 1, or 2).
//!    - The voter selects the majority decision (2 out of 3 match) as the correct output.
//!    - The voter logs a warning to let the kernel dispatcher re-synchronize or restart the faulty runner.
//! 5. If all three outputs are different, the voter raises a catastrophic system failure alert.

use alloc::string::String;
use alloc::format;

/// The health and redundancy status of a TMR execution run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmrStatus {
    /// Perfectly redundant execution. All three runners matched.
    Perfect,
    /// Divergence detected! One runner produced a corrupt value, but was corrected by majority vote.
    Corrected {
        /// The index of the runner that diverged (0, 1, or 2).
        faulty_runner: usize,
        /// Description of the faulty value.
        faulty_value: String,
        /// Description of the correct majority value that was chosen.
        majority_value: String,
    },
    /// Catastrophic error: All three runners produced divergent values. No majority could be reached.
    Failed,
}

/// The result returned by the TMR voting engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmrResult<T> {
    /// The correct output value resolved by majority vote (if successful).
    pub value: Option<T>,
    /// The diagnosis status of the voting process.
    pub status: TmrStatus,
}

/// TMR Voting and Redundant Execution Engine.
pub struct TmrVoter;

impl TmrVoter {
    /// Performs majority voting on three values.
    ///
    /// If a single value diverges, it corrects the output and details the faulty runner.
    pub fn vote<T: Clone + PartialEq + core::fmt::Debug>(r0: T, r1: T, r2: T) -> TmrResult<T> {
        if r0 == r1 && r1 == r2 {
            TmrResult {
                value: Some(r0),
                status: TmrStatus::Perfect,
            }
        } else if r0 == r1 {
            // Runner 2 diverged
            let faulty = format!("{:?}", r2);
            let majority = format!("{:?}", r0);
            TmrResult {
                value: Some(r0),
                status: TmrStatus::Corrected {
                    faulty_runner: 2,
                    faulty_value: faulty,
                    majority_value: majority,
                },
            }
        } else if r0 == r2 {
            // Runner 1 diverged
            let faulty = format!("{:?}", r1);
            let majority = format!("{:?}", r0);
            TmrResult {
                value: Some(r0),
                status: TmrStatus::Corrected {
                    faulty_runner: 1,
                    faulty_value: faulty,
                    majority_value: majority,
                },
            }
        } else if r1 == r2 {
            // Runner 0 diverged
            let faulty = format!("{:?}", r0);
            let majority = format!("{:?}", r1);
            TmrResult {
                value: Some(r1),
                status: TmrStatus::Corrected {
                    faulty_runner: 0,
                    faulty_value: faulty,
                    majority_value: majority,
                },
            }
        } else {
            // All three differ! Catastrophe.
            TmrResult {
                value: None,
                status: TmrStatus::Failed,
            }
        }
    }

    /// Executes a critical mathematical task redundantly three times and votes on the result.
    ///
    /// Passes the runner index (0, 1, or 2) to the task closure so that the caller can
    /// simulate hardware register corruptions or faults in specific runners for test validation.
    pub fn execute_redundant<T, F>(task: F) -> TmrResult<T>
    where
        T: Clone + PartialEq + core::fmt::Debug,
        F: Fn(usize) -> T,
    {
        let r0 = task(0);
        let r1 = task(1);
        let r2 = task(2);

        Self::vote(r0, r1, r2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmr_perfect_match() {
        let res = TmrVoter::vote(42, 42, 42);
        assert_eq!(res.value, Some(42));
        assert_eq!(res.status, TmrStatus::Perfect);
    }

    #[test]
    fn test_tmr_single_divergence_corrected() {
        // Test runner 0 diverging
        let res0 = TmrVoter::vote(999, 42, 42);
        assert_eq!(res0.value, Some(42));
        assert!(matches!(
            res0.status,
            TmrStatus::Corrected {
                faulty_runner: 0,
                ..
            }
        ));

        // Test runner 1 diverging
        let res1 = TmrVoter::vote(42, 999, 42);
        assert_eq!(res1.value, Some(42));
        assert!(matches!(
            res1.status,
            TmrStatus::Corrected {
                faulty_runner: 1,
                ..
            }
        ));

        // Test runner 2 diverging
        let res2 = TmrVoter::vote(42, 42, 999);
        assert_eq!(res2.value, Some(42));
        assert!(matches!(
            res2.status,
            TmrStatus::Corrected {
                faulty_runner: 2,
                ..
            }
        ));
    }

    #[test]
    fn test_tmr_catastrophic_failure() {
        let res = TmrVoter::vote(1, 2, 3);
        assert_eq!(res.value, None);
        assert_eq!(res.status, TmrStatus::Failed);
    }

    #[test]
    fn test_tmr_simulated_computation_flip() {
        // Simulate a mathematical operation: x * 2.
        // Inject a simulated bit flip in runner 1.
        let val = 10;
        let res = TmrVoter::execute_redundant(|idx| {
            let mut result = val * 2;
            if idx == 1 {
                result ^= 1 << 3; // Corrupt runner 1 by flipping bit 3 (value shifts from 20 to 28)
            }
            result
        });

        assert_eq!(res.value, Some(20)); // Voter should successfully recover the correct majority result
        match res.status {
            TmrStatus::Corrected { faulty_runner, .. } => {
                assert_eq!(faulty_runner, 1);
            }
            _ => panic!("Expected corrected status"),
        }
    }
}
