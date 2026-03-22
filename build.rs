fn main() -> Result<(), Box<dyn std::error::Error>> {
    build_info::set_cargo_workspace_members_env();

    Ok(())
}
