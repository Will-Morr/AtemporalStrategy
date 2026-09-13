fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schemars::schema_for!(atemporal_contracts::ContractCatalog);
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}
