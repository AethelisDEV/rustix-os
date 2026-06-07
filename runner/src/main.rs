use std::path::PathBuf;
use std::process::Command;
use anyhow::{anyhow, Context, Result};

fn main() -> Result<()> {
    // 1. Determine compilation profile (debug vs release)
    let is_release = std::env::args().any(|arg| arg == "--release");
    let profile = if is_release { "release" } else { "debug" };

    println!("====================================================");
    println!("🚀 AE Rustanium Host-Side UEFI/BIOS Workspace Builder");
    println!("====================================================");

    // 2. Resolve workspace directories
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")
        .context("Failed to get CARGO_MANIFEST_DIR environment variable")?);
    let mut workspace_root = manifest_dir.parent()
        .ok_or_else(|| anyhow!("Failed to resolve workspace root directory"))?
        .to_path_buf();

    // Handle potential AE Rustanium -> Rustix OS directory rename/mismatch
    if !workspace_root.exists() {
        let root_str = workspace_root.to_string_lossy().replace("AE Rustanium", "Rustix OS");
        let alt_root = PathBuf::from(&root_str);
        if alt_root.exists() {
            workspace_root = alt_root;
        }
    }
    let target_dir = workspace_root.join("target");

    // Redirect temp directories to workspace drive to prevent disk space issues on C:
    let custom_temp_dir = target_dir.join("tmp");
    std::fs::create_dir_all(&custom_temp_dir)
        .context("Failed to create custom temp directory")?;
    std::env::set_var("TEMP", &custom_temp_dir);
    std::env::set_var("TMP", &custom_temp_dir);

    println!("📂 Workspace Root : {}", workspace_root.display());
    println!("🛠️ Target Profile  : {}", profile);
    println!("🧹 Redirected TEMP/TMP to: {}", custom_temp_dir.display());

    // 3. Compile the usermode-desktop package for x86_64-unknown-none
    // Use 'cargo rustc' to pass additional compiler flags for static linking
    // and custom linker script placement at VA 0x400000.
    println!("\n📦 Step 0: Compiling usermode-desktop target...");
    let mut build_desktop = Command::new("cargo");
    build_desktop.current_dir(&workspace_root);
    build_desktop.args([
        "+nightly",
        "rustc",
        "--package", "usermode-desktop",
        "--target", "x86_64-unknown-none",
    ]);
    if is_release {
        build_desktop.arg("--release");
    }
    // Pass linker script and static relocation model via rustc flags
    build_desktop.args([
        "--", "-C", "relocation-model=static", "-C", "link-arg=-Tlinker.ld",
    ]);

    let desktop_status = build_desktop.status()
        .context("Failed to execute cargo build for usermode-desktop")?;
    if !desktop_status.success() {
        return Err(anyhow!("usermode-desktop compilation failed"));
    }

    // Convert compiled ELF to flat binary via llvm-objcopy
    println!("💾 Step 0.5: Converting usermode-desktop ELF to flat binary...");
    let desktop_elf = target_dir
        .join("x86_64-unknown-none")
        .join(profile)
        .join("usermode-desktop");
    let desktop_bin = target_dir.join("usermode-desktop.bin");

    // Find llvm-objcopy dynamically
    let objcopy_path = {
        let mut path = None;
        if let (Ok(rustup_home), Ok(toolchain)) = (std::env::var("RUSTUP_HOME"), std::env::var("RUSTUP_TOOLCHAIN")) {
            let triple = if toolchain.contains("x86_64-pc-windows-msvc") {
                Some("x86_64-pc-windows-msvc")
            } else if toolchain.contains("x86_64-pc-windows-gnu") {
                Some("x86_64-pc-windows-gnu")
            } else {
                None
            };
            if let Some(t) = triple {
                let p = PathBuf::from(&rustup_home)
                    .join("toolchains")
                    .join(&toolchain)
                    .join("lib")
                    .join("rustlib")
                    .join(t)
                    .join("bin")
                    .join("llvm-objcopy.exe");
                if p.exists() {
                    path = Some(p);
                }
            }
        }
        if path.is_none() {
            if let Ok(user_profile) = std::env::var("USERPROFILE") {
                for toolchain in &["nightly-x86_64-pc-windows-msvc", "nightly-x86_64-pc-windows-gnu"] {
                    let triple = if toolchain.contains("msvc") { "x86_64-pc-windows-msvc" } else { "x86_64-pc-windows-gnu" };
                    let p = PathBuf::from(&user_profile)
                        .join(".rustup")
                        .join("toolchains")
                        .join(toolchain)
                        .join("lib")
                        .join("rustlib")
                        .join(triple)
                        .join("bin")
                        .join("llvm-objcopy.exe");
                    if p.exists() {
                        path = Some(p);
                        break;
                    }
                }
            }
        }
        path.unwrap_or_else(|| PathBuf::from("llvm-objcopy.exe"))
    };

    println!("🔧 Using llvm-objcopy at: {}", objcopy_path.display());
    let mut objcopy = Command::new(&objcopy_path);
    objcopy.args([
        "-O", "binary",
        &desktop_elf.to_string_lossy(),
        &desktop_bin.to_string_lossy(),
    ]);

    let objcopy_status = objcopy.status()
        .context("Failed to run llvm-objcopy. Make sure Rust nightly MSVC is installed.")?;
    if !objcopy_status.success() {
        return Err(anyhow!("llvm-objcopy failed to generate flat binary"));
    }
    println!("✅ Flat binary generated at: {}", desktop_bin.display());

    // 4. Compile the bare-metal kernel package for x86_64-unknown-none
    println!("\n📦 Step 1: Compiling kernel-x86 target...");
    let mut build_cmd = Command::new("cargo");
    build_cmd.current_dir(&workspace_root);
    build_cmd.args([
        "+nightly",
        "build",
        "--package", "kernel-x86",
        "--target", "x86_64-unknown-none",
        "--bin", "kernel-x86",
    ]);
    if is_release {
        build_cmd.arg("--release");
    }

    let build_status = build_cmd.status()
        .context("Failed to execute cargo build command")?;
    if !build_status.success() {
        return Err(anyhow!("Kernel compilation failed with status: {:?}", build_status.code()));
    }
    println!("✅ Kernel compiled successfully!");

    // 4. Resolve the compiled kernel ELF binary path
    let kernel_elf_path = target_dir
        .join("x86_64-unknown-none")
        .join(profile)
        .join("kernel-x86");

    if !kernel_elf_path.exists() {
        return Err(anyhow!("Compiled kernel binary not found at: {}", kernel_elf_path.display()));
    }

    // 5. Generate the BIOS bootable disk image
    println!("\n💾 Step 2: Creating legacy BIOS bootable image...");
    let bios_image_path = target_dir
        .join("x86_64-unknown-none")
        .join(profile)
        .join("bios.img");

    let bios_boot = bootloader::BiosBoot::new(&kernel_elf_path);
    bios_boot.create_disk_image(&bios_image_path)
        .context("Failed to generate BIOS boot image")?;
    println!("✅ BIOS boot image created at: {}", bios_image_path.display());

    // 6. Generate the modern UEFI bootable disk image (for your Ryzen 7500F physical boot!)
    println!("\n🔌 Step 3: Creating modern UEFI bootable image...");
    let uefi_image_path = target_dir
        .join("x86_64-unknown-none")
        .join(profile)
        .join("uefi.img");

    let uefi_boot = bootloader::UefiBoot::new(&kernel_elf_path);
    uefi_boot.create_disk_image(&uefi_image_path)
        .context("Failed to generate UEFI boot image")?;
    println!("✅ UEFI boot image created at: {}", uefi_image_path.display());
    println!("💡 [PRO TIP] Write this file to a USB stick (e.g. via Rufus in DD mode) to boot your physical Ryzen 7500F!");

    // 7. Execute QEMU on the BIOS image with COM1 Serial mapped directly to stdout
    println!("\n🛸 Step 4: Spawning QEMU bare-metal environment (BIOS emulation)...");
    let mut qemu = Command::new("C:\\Program Files\\qemu\\qemu-system-x86_64.exe");
    qemu.args([
        "-drive", &format!("format=raw,file={}", bios_image_path.display()),
        "-serial", "stdio", // Direct COM1 print to console!
        "-m", "1G",
    ]);

    let mut qemu_child = qemu.spawn()
        .context("Failed to execute QEMU. Ensure QEMU is installed at 'C:\\Program Files\\qemu\\qemu-system-x86_64.exe'")?;
    
    println!("🖥️ QEMU Window active. Logs are running on this console below.");
    println!("------------------------------------------------------------\n");

    let exit_status = qemu_child.wait()
        .context("Failed to wait for QEMU execution")?;

    println!("\n------------------------------------------------------------");
    println!("🔌 QEMU exited with status: {:?}", exit_status);
    Ok(())
}
