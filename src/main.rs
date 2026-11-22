//! SigmaOS component binary: zfs-integration

use sigmaos_zfs-integration::Component;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Component::new().operate()?;
    println!("sigmaos-zfs-integration: ok");
    Ok(())
}
