#![no_std]


// Copyright 2026 Aethelis (https://github.com/AethelisDEV)
// 
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// 
//     http://www.apache.org/licenses/LICENSE-2.0
// 
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.


//! # AE Rustanium Kernel Core Crate
//!
//! This crate provides the main operating system bootstrap, clock ticks coordinator,
//! microkernel dynamic module orchestration, and diagnostic metrics gathering.
//!
//! Fully written under a **Zero Unsafe Policy**.

extern crate alloc;

pub mod bootstrap;
pub mod hal;

// Re-export core types for simplified external usage
pub use bootstrap::SystemCore;
