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
    let workspace_root = manifest_dir.parent()
        .ok_or_else(|| anyhow!("Failed to resolve workspace root directory"))?;
    let target_dir = workspace_root.join("target");

    println!("📂 Workspace Root : {}", workspace_root.display());
    println!("🛠️ Target Profile  : {}", profile);

    // 3. Compile the bare-metal kernel package for x86_64-unknown-none
    println!("\n📦 Step 1: Compiling kernel-x86 target...");
    let mut build_cmd = Command::new("cargo");
    build_cmd.current_dir(workspace_root);
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
