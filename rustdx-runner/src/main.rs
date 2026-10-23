use std::{path::PathBuf, time::Duration};

use argh::FromArgs;
use rustdx_runner::{
    Error, Result, config::Config, consumer::PinnedDataset, ledger::Ledger, ops, pipeline,
};

#[derive(FromArgs)]
/// Single-host production collection, immutable publication and recovery.
struct Args {
    /// run, recover, status, metrics, catalog, export, backup or restore
    #[argh(positional)]
    action: String,
    /// strict JSON job configuration (run)
    #[argh(option)]
    config: Option<PathBuf>,
    /// controller root (all actions except run)
    #[argh(option)]
    root: Option<PathBuf>,
    /// new backup/restore destination
    #[argh(option)]
    destination: Option<PathBuf>,
    /// dataset for a fixed catalog
    #[argh(option)]
    dataset: Option<String>,
    /// pin a historical catalog version
    #[argh(option)]
    version: Option<u64>,
    /// versioned database target for export-database
    #[argh(option)]
    target: Option<String>,
    /// first date of an export
    #[argh(option)]
    from: Option<chrono::NaiveDate>,
    /// last date of an export
    #[argh(option)]
    through: Option<chrono::NaiveDate>,
    /// atomic CSV export destination outside the controller root
    #[argh(option)]
    output: Option<PathBuf>,
    /// export row budget
    #[argh(option, default = "10000000")]
    max_rows: u64,
    /// unfinished attempt to quarantine after investigation
    #[argh(option)]
    attempt: Option<String>,
    /// explicit quarantine reason
    #[argh(option)]
    reason: Option<String>,
    /// byte budget for backup or restore
    #[argh(option, default = "10737418240")]
    max_bytes: u64,
    /// operation timeout for administrative actions (seconds)
    #[argh(option, default = "3600")]
    timeout: u64,
    /// cooperative cancellation marker
    #[argh(option)]
    cancel_file: Option<PathBuf>,
}

fn required<T>(value: Option<T>, name: &str) -> Result<T> {
    value.ok_or_else(|| Error::Invalid(format!("missing --{name}")))
}

fn main() -> Result<()> {
    let args: Args = argh::from_env();
    if args.action == "run" {
        let config = Config::load(&required(args.config, "config")?)?;
        let operation = pipeline::operation(&config, args.cancel_file)?;
        let signal = operation.clone();
        ctrlc::set_handler(move || signal.cancel()).map_err(|e| Error::Invalid(e.to_string()))?;
        let _scope = operation.enter();
        println!(
            "{}",
            serde_json::to_string_pretty(&pipeline::execute(&config, &operation)?)?
        );
        return Ok(());
    }
    let root = required(args.root, "root")?;
    let operation = rustdx_complete::operation::OperationContext::new(
        Duration::from_secs(args.timeout),
        args.cancel_file,
    )?;
    let signal = operation.clone();
    ctrlc::set_handler(move || signal.cancel()).map_err(|e| Error::Invalid(e.to_string()))?;
    let _scope = operation.enter();
    match args.action.as_str() {
        "recover" => println!(
            "{}",
            serde_json::to_string_pretty(&pipeline::recover(&root)?)?
        ),
        "quarantine" => pipeline::quarantine(
            &root,
            &required(args.attempt, "attempt")?,
            &required(args.reason, "reason")?,
        )?,
        "backup" => ops::backup(
            &root,
            &required(args.destination, "destination")?,
            args.max_bytes,
        )?,
        "restore" => ops::restore(
            &root,
            &required(args.destination, "destination")?,
            args.max_bytes,
        )?,
        "status" => println!(
            "{}",
            serde_json::to_string_pretty(&Ledger::open(&root.join("ledger.sqlite"))?.statuses()?)?
        ),
        "metrics" => {
            let mut text = ops::metrics(&Ledger::open(&root.join("ledger.sqlite"))?)?;
            text.push_str(&format!(
                "# TYPE rustdx_root_free_bytes gauge\nrustdx_root_free_bytes {}\n",
                rustdx_runner::storage::free_bytes(&root)?
            ));
            if let Some(output) = args.output {
                use std::io::Write;
                let (atomic, mut file) = rustdx_complete::atomic_file::AtomicFile::create(&output)?;
                file.write_all(text.as_bytes())?;
                rustdx_complete::operation::check().map_err(|_| Error::Cancelled)?;
                drop(file);
                atomic.commit()?;
            } else {
                print!("{text}");
            }
        }
        "catalog" => {
            let ledger = Ledger::open(&root.join("ledger.sqlite"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&pipeline::fixed_catalog(
                    &root,
                    &ledger,
                    &required(args.dataset, "dataset")?,
                    args.version
                )?)?
            );
        }
        "export" | "export-database" => {
            let pinned =
                PinnedDataset::open(&root, &required(args.dataset, "dataset")?, args.version)?;
            let from = required(args.from, "from")?;
            let through = required(args.through, "through")?;
            let output = required(args.output, "output")?;
            let rows = if args.action == "export-database" {
                pinned.export_database(
                    from,
                    through,
                    &output,
                    args.max_rows,
                    args.max_bytes,
                    &required(args.target, "target")?,
                )?
            } else {
                pinned.export(from, through, &output, args.max_rows, args.max_bytes)?
            };
            println!(
                "{}",
                serde_json::json!({"catalog_version":pinned.catalog.version,"rows":rows})
            );
        }
        _ => return Err(Error::Invalid("unknown action".into())),
    }
    Ok(())
}
