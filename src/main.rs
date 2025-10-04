use anyhow::{Context, Result};
use clap::Parser;
use log::{debug, error, info, trace, warn};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

    /// Skip hidden files and directories
    #[arg(long, default_value_t = true)]
    skip_hidden: bool,
}

fn main() {
    // Initialize the logger
    env_logger::init();

    let args = Args::parse();

    info!("Starting CRLF to LF conversion");
    info!("Options: recursive={}, dry_run={}, skip_hidden={}", 
          args.recursive, args.dry_run, args.skip_hidden);
    info!("Processing {} path(s)", args.paths.len());

    let mut total_files = 0;
    let mut converted_files = 0;
    let mut errors = 0;

    // Process each path provided
    for path in &args.paths {
        trace!("Processing path: {:?}", path);

        match process_path(path, &args, &mut total_files, &mut converted_files) {
            Ok(_) => {
                trace!("Successfully processed path: {:?}", path);
            }
            Err(e) => {
                error!("Failed to process path {:?}: {}", path, e);
                errors += 1;
            }
        }
    }

    // Summary
    info!("Conversion complete");
    info!("Total text files processed: {}", total_files);
    info!("Files converted: {}", converted_files);

    if errors > 0 {
        warn!("Errors encountered: {}", errors);
        std::process::exit(1);
    }
}

/// Process a single path (file or directory)
fn process_path(
    path: &Path,
    args: &Args,
    total_files: &mut usize,
    converted_files: &mut usize,
) -> Result<()> {
    if !path.exists() {
        return Err(anyhow::anyhow!("Path does not exist: {:?}", path));
    }

    if path.is_file() {
        trace!("Path is a file: {:?}", path);
        if let Err(e) = process_file(path, args.dry_run, total_files, converted_files) {
            error!("Error processing file {:?}: {}", path, e);
        }
    } else if path.is_dir() {
        trace!("Path is a directory: {:?}", path);
        process_directory(path, args, total_files, converted_files)?;
    } else {
        warn!("Path is neither a file nor a directory: {:?}", path);
    }

    Ok(())
}

/// Process all files in a directory
fn process_directory(
    dir: &Path,
    args: &Args,
    total_files: &mut usize,
    converted_files: &mut usize,
) -> Result<()> {
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

                // Skip hidden files/directories if requested
                if args.skip_hidden && is_hidden(path) {
                    trace!("Skipping hidden path: {:?}", path);
                    continue;
                }

                // Skip directories themselves, we only process files
                if path.is_file() {
                    trace!("Found file in directory: {:?}", path);
                    if let Err(e) = process_file(path, args.dry_run, total_files, converted_files) {
                        error!("Error processing file {:?}: {}", path, e);
                    }
                }
            }
            Err(e) => {
                warn!("Error walking directory entry: {}", e);
            }
        }
    }

    Ok(())
}

/// Check if a path is hidden (starts with a dot)
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

/// Process a single file
fn process_file(
    path: &Path,
    dry_run: bool,
    total_files: &mut usize,
    converted_files: &mut usize,
) -> Result<()> {
    trace!("Checking if file is text: {:?}", path);

    // Check if file is a text file
    if !is_text_file(path)? {
        trace!("File is not a text file, skipping: {:?}", path);
        return Ok(());
    }

    debug!("Processing text file: {:?}", path);
    *total_files += 1;

    // Read file content
    let content = fs::read(path)
        .with_context(|| format!("Failed to read file: {:?}", path))?;

    trace!("Read {} bytes from file", content.len());

    // Check if file contains CRLF
    if !content.windows(2).any(|w| w == b"\r\n") {
        trace!("File does not contain CRLF, skipping: {:?}", path);
        return Ok(());
    }

    debug!("File contains CRLF line endings: {:?}", path);

    // Convert CRLF to LF
    let converted = convert_crlf_to_lf(&content);

    if converted.len() == content.len() {
        trace!("No changes after conversion (already LF only): {:?}", path);
        return Ok(());
    }

    let bytes_saved = content.len() - converted.len();
    debug!("Conversion will reduce file size by {} bytes", bytes_saved);

    if dry_run {
        info!("[DRY RUN] Would convert: {:?} ({} bytes saved)", path, bytes_saved);
    } else {
        // Write converted content back to file
        fs::write(path, &converted)
            .with_context(|| format!("Failed to write file: {:?}", path))?;

        info!("Converted: {:?} ({} bytes saved)", path, bytes_saved);
    }

    *converted_files += 1;
    Ok(())
}

/// Check if a file is a text file using file-format crate
fn is_text_file(path: &Path) -> Result<bool> {
    trace!("Detecting file format for: {:?}", path);

    // Try to determine file format
    match file_format::FileFormat::from_file(path) {
        Ok(format) => {
            let media_type = format.media_type();
            trace!("Detected media type: {}", media_type);

            // Check if media type starts with "text/"
            let is_text = media_type.starts_with("text/");

            // Also accept some common programming file extensions
            // that might not be detected as text
            let is_code = matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("rs" | "toml" | "json" | "yaml" | "yml" | "md" | "txt" | 
                     "sh" | "py" | "js" | "ts" | "c" | "cpp" | "h" | "hpp" |
                     "java" | "go" | "rb" | "php" | "cs" | "swift" | "kt")
            );

            trace!("Is text: {}, Is code: {}", is_text, is_code);
            Ok(is_text || is_code)
        }
        Err(e) => {
            trace!("Failed to detect file format: {}", e);
            // If detection fails, check by extension as fallback
            let is_code = matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("rs" | "toml" | "json" | "yaml" | "yml" | "md" | "txt" | 
                     "sh" | "py" | "js" | "ts" | "c" | "cpp" | "h" | "hpp" |
                     "java" | "go" | "rb" | "php" | "cs" | "swift" | "kt")
            );

            if is_code {
                trace!("Treating as text based on extension");
                Ok(true)
            } else {
                trace!("Cannot determine if file is text, skipping");
                Ok(false)
            }
        }
    }
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

    #[test]
    fn test_is_hidden() {
        assert!(is_hidden(Path::new(".hidden")));
        assert!(is_hidden(Path::new(".gitignore")));
        assert!(!is_hidden(Path::new("visible.txt")));
        assert!(!is_hidden(Path::new("file")));
    }
}
