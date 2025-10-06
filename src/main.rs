use anyhow::{Context, Result};
use binaryornot::is_binary;
use clap::Parser;
use log::{debug, error, info, trace, warn};
use walkdir::WalkDir;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A CLI tool to convert CRLF line endings to LF in text files
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Paths to files or directories to process
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    /// Process directories recursively
    #[arg(short, long, default_value_t = false)]
    recursive: bool,

    /// Dry run mode - show what would be changed without modifying files
    #[arg(short = 'n', long, default_value_t = false)]
    dry_run: bool,
}

fn main() {
    // Initialize the logger
    env_logger::init();

    let args = Args::parse();

    info!("Starting CRLF to LF conversion");
    info!("Options: recursive={recursive}, dry_run={dry_run}", recursive = args.recursive, dry_run = args.dry_run);
    info!("Processing {} path(s)", args.paths.len());

    let mut total_files = 0;
    let mut converted_files = 0;
    let mut errors = 0;

    // Process each path provided
    for path in &args.paths {
        debug!("Processing path: {}", path.display());

        match process_path(path, &args, &mut total_files, &mut converted_files) {
            Ok(_) => {
                info!("Processed path: {}", path.display());
            }
            Err(e) => {
                error!("Failed to process path {path}: {e}", path = path.display());
                errors += 1;
            }
        }
    }

    // Summary
    info!("Conversion complete");
    info!("Total text files processed: {total_files}");
    info!("Files converted: {converted_files}");

    if errors > 0 {
        warn!("Errors encountered: {errors}");
        std::process::exit(1);
    }
}

/// Process a single path (file or directory)
fn process_path(path: &Path, args: &Args, total_files: &mut usize, converted_files: &mut usize) -> Result<()> {
    if !path.exists() {
        return Err(anyhow::anyhow!("Path does not exist: {}", path.display()));
    }

    if path.is_file() {
        trace!("Path is a file: {}", path.display());
        if let Err(e) = process_file(path, args.dry_run, total_files, converted_files) {
            error!("Error processing file {path}: {e}", path = path.display());
        }
    } else if path.is_dir() {
        trace!("Path is a directory: {}", path.display());
        process_directory(path, args, total_files, converted_files)?;
    } else {
        warn!("Path is neither a file nor a directory: {}", path.display());
    }

    Ok(())
}

/// Process all files in a directory
fn process_directory(dir: &Path, args: &Args, total_files: &mut usize, converted_files: &mut usize) -> Result<()> {
    debug!("Processing directory: {:?}", dir);

    let walker = if args.recursive {
        trace!("Walking directory recursively");
        WalkDir::new(dir).follow_links(false)
    } else {
        trace!("Walking directory non-recursively (max_depth=1)");
        WalkDir::new(dir).max_depth(1).follow_links(false)
    };

    for entry in walker {
        match entry {
            Ok(entry) => {
                let path = entry.path();

                // Skip directories themselves, we only process files
                if path.is_file() {
                    trace!("Found file in directory: {}", path.display());
                    if let Err(e) = process_file(path, args.dry_run, total_files, converted_files) {
                        error!("Error processing file {path}: {e}", path = path.display());
                    }
                }
            }
            Err(e) => {
                warn!("Error walking directory entry: {e}");
            }
        }
    }

    Ok(())
}

/// Process a single file
fn process_file(path: &Path, dry_run: bool, total_files: &mut usize, converted_files: &mut usize) -> Result<()> {
    // Check if this file is a text file
    trace!("Checking if file is text: {}", path.display());
    if is_binary(path)? {
        trace!("File is not a text file, skipping: {}", path.display());
        return Ok(());
    }

    debug!("Processing text file: {}", path.display());
    *total_files += 1;

    // Read file content
    let content = fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))?;
    trace!("Read {} bytes from file", content.len());

    // Check if the file contains CRLF
    if !content.windows(2).any(|w| w == b"\r\n") {
        trace!("File does not contain CRLF, skipping: {}", path.display());
        return Ok(());
    }

    debug!("File contains CRLF line endings: {}", path.display());

    // Convert CRLF to LF
    let converted = convert_crlf_to_lf(&content);

    if converted.len() == content.len() {
        trace!("No changes after conversion (already LF only): {}", path.display());
        return Ok(());
    }

    let bytes_saved = content.len() - converted.len();
    debug!("Conversion will reduce file size by {bytes_saved} bytes");

    if dry_run {
        info!(
            "[DRY RUN] Would convert: {path} ({bytes_saved} bytes saved)",
            path = path.display()
        );
    } else {
        // Write converted content back to the file
        fs::write(path, &converted).with_context(|| format!("Failed to write file: {}", path.display()))?;

        debug!("Converted: {path} ({bytes_saved} bytes saved)", path = path.display());
    }

    *converted_files += 1;
    Ok(())
}

/// Convert CRLF line endings to LF
fn convert_crlf_to_lf(content: &[u8]) -> Vec<u8> {
    trace!("Converting CRLF to LF in {} byte buffer", content.len());

    let mut result = Vec::with_capacity(content.len());
    let mut i = 0;

    while i < content.len() {
        if i + 1 < content.len() && content[i] == b'\r' && content[i + 1] == b'\n' {
            // Found CRLF, replace with LF
            result.push(b'\n');
            i += 2;
        } else {
            // Regular character
            result.push(content[i]);
            i += 1;
        }
    }

    trace!("Conversion complete, result size: {} bytes", result.len());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_crlf_to_lf() {
        let input = b"Hello\r\nWorld\r\nTest\r\n";
        let expected = b"Hello\nWorld\nTest\n";
        let result = convert_crlf_to_lf(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_convert_no_crlf() {
        let input = b"Hello\nWorld\nTest\n";
        let expected = b"Hello\nWorld\nTest\n";
        let result = convert_crlf_to_lf(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_convert_mixed() {
        let input = b"Hello\r\nWorld\nTest\r\n";
        let expected = b"Hello\nWorld\nTest\n";
        let result = convert_crlf_to_lf(input);
        assert_eq!(result, expected);
    }
}
