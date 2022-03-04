use move_binary_format::errors::Location;
use move_binary_format::CompiledModule;
use move_cli::sandbox::utils::PackageContext;
use move_cli::Move;
use move_command_line_common::files::MOVE_EXTENSION;
use move_core_types::account_address::AccountAddress;
use move_package::source_package::layout::SourcePackageLayout;
use starcoin_config::BuiltinNetworkID;
use starcoin_transactional_test_harness::remote_state::RemoteStateView;
use std::collections::BTreeMap;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct DecompileCommand {
    #[structopt(name = "rpc", long)]
    /// use remote starcoin rpc as initial state.
    rpc: Option<String>,
    #[structopt(long = "block-number", requires("rpc"))]
    /// block number to read state from. default to latest block number.
    block_number: Option<u64>,

    #[structopt(long = "network", short, conflicts_with("rpc"))]
    /// built in network id, like main, barnard
    network: Option<BuiltinNetworkID>,

    package_address: AccountAddress,
    // #[structopt(long = "named-address")]
    // package_address_alias: Option<String>,
}

pub fn handle_decompile(move_args: &Move, cmd: DecompileCommand) -> anyhow::Result<()> {
    let pkg_ctx = PackageContext::new(&move_args.package_path, &move_args.build_config)?;
    let pkg = pkg_ctx.package();
    // let resolved_graph = move_args.build_config.clone().resolution_graph_for_package(&move_args.package_path)?;
    // let name_address_mapping = resolved_graph.get_package(&resolved_graph.root_package.package.name).resolution_table.iter().map(|(k, v)| {
    //     (*v, k.as_str().to_string())
    // }).collect::<BTreeMap<_, _>>();
    let named_address_mapping: BTreeMap<_, String> = pkg
        .compiled_package_info
        .module_resolution_metadata
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().to_string()))
        .collect();
    let source_path = SourcePackageLayout::try_find_root(&move_args.package_path)?
        .join(SourcePackageLayout::Sources.path());

    let rpc = cmd.rpc.unwrap_or_else(|| {
        format!(
            "http://{}:{}",
            cmd.network.unwrap().boot_nodes_domain(),
            9850
        )
    });

    let remote_view = RemoteStateView::from_url(&rpc, cmd.block_number).unwrap();
    let modules = remote_view
        .get_modules(cmd.package_address)
        .map_err(|e| e.into_vm_status())?
        .unwrap_or_default();

    for (module_name, module_data) in modules {
        let module = CompiledModule::deserialize(&module_data)
            .map_err(|v| v.finish(Location::Undefined).into_vm_status())?;
        let (_, source) = move_compiler::interface_generator::write_module_to_string(
            &named_address_mapping,
            &module,
        )?;

        std::fs::write(
            {
                let mut p = source_path.join(module_name.as_str());
                p.set_extension(MOVE_EXTENSION);
                p
            },
            source,
        )?;
    }

    Ok(())
}
