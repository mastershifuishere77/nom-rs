use std::env;
use std::fs::File;
use std::io::{self, BufWriter, Write};

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut num_drvs: usize = 100;
    let mut logs_per_drv: usize = 50;
    let mut num_downloads: usize = 50;
    let mut output_file: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--drvs" => {
                i += 1;
                if i < args.len() {
                    num_drvs = args[i].parse().unwrap_or(100);
                }
            }
            "--logs" => {
                i += 1;
                if i < args.len() {
                    logs_per_drv = args[i].parse().unwrap_or(50);
                }
            }
            "--downloads" => {
                i += 1;
                if i < args.len() {
                    num_downloads = args[i].parse().unwrap_or(50);
                }
            }
            "--output" | "-o" => {
                i += 1;
                if i < args.len() {
                    output_file = Some(args[i].clone());
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: bench-gen [--drvs N] [--logs N] [--downloads N] [-o output.json]"
                );
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    let out: Box<dyn Write> = match output_file {
        Some(path) => Box::new(BufWriter::with_capacity(65536, File::create(path)?)),
        None => Box::new(BufWriter::with_capacity(65536, io::stdout())),
    };
    let mut writer = out;

    // Header messages
    writeln!(
        writer,
        "@nix {{\"action\":\"msg\",\"level\":4,\"msg\":\"evaluating derivations\"}}"
    )?;
    writeln!(
        writer,
        "@nix {{\"action\":\"msg\",\"level\":3,\"msg\":\"these {} derivations will be built:\"}}",
        num_drvs
    )?;

    // Planned builds
    for idx in 0..num_drvs {
        writeln!(
            writer,
            "@nix {{\"action\":\"msg\",\"level\":3,\"msg\":\"  /nix/store/{:032x}-package-{:04}.drv\"}}",
            idx * 7919,
            idx
        )?;
    }

    let mut act_id: u64 = 100_000;

    // Simulate parallel downloads
    for d in 0..num_downloads {
        act_id += 1;
        let id = act_id;
        let host = if d % 2 == 0 {
            "https://cache.nixos.org"
        } else {
            "https://cuda-maintainers.cachix.org"
        };
        writeln!(
            writer,
            "@nix {{\"action\":\"start\",\"id\":{},\"level\":0,\"parent\":0,\"text\":\"fetching path '/nix/store/{:032x}-download-{:04}' from '{}'\",\"type\":104}}",
            id,
            d * 31337,
            d,
            host
        )?;
        // Progress updates
        let total_bytes = 10_000_000 + (d * 500_000);
        writeln!(
            writer,
            "@nix {{\"action\":\"result\",\"fields\":[{},{},0,0],\"id\":{},\"type\":105}}",
            total_bytes / 2,
            total_bytes,
            id
        )?;
        writeln!(
            writer,
            "@nix {{\"action\":\"result\",\"fields\":[{},{},0,0],\"id\":{},\"type\":105}}",
            total_bytes,
            total_bytes,
            id
        )?;
        writeln!(writer, "@nix {{\"action\":\"stop\",\"id\":{}}}", id)?;
    }

    // Simulate builds with compiler logs
    for idx in 0..num_drvs {
        act_id += 1;
        let id = act_id;
        let drv_path = format!("/nix/store/{:032x}-package-{:04}.drv", idx * 7919, idx);
        writeln!(
            writer,
            "@nix {{\"action\":\"start\",\"id\":{},\"level\":0,\"parent\":0,\"text\":\"building '{}'\",\"type\":105}}",
            id, drv_path
        )?;

        // Output lines of compiler logs
        for l in 0..logs_per_drv {
            writeln!(
                writer,
                "@nix {{\"action\":\"result\",\"fields\":[\"gcc -O2 -Wall -c src/module_{:03}.c -o src/module_{:03}.o\"],\"id\":{},\"type\":101}}",
                l, l, id
            )?;
        }

        // Complete the build
        writeln!(writer, "@nix {{\"action\":\"stop\",\"id\":{}}}", id)?;
    }

    writeln!(
        writer,
        "@nix {{\"action\":\"msg\",\"level\":3,\"msg\":\"build complete\"}}"
    )?;

    writer.flush()?;
    Ok(())
}
