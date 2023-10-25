
#[derive(Debug, serde::Deserialize)]
pub struct TenantCreateRequest {
    pub name: String,
    // base 64 encoded public key for the merchant
    pub public_key: String

}


#[allow(dead_code)]
pub struct TenantCreateResponse {
    pub tenant_id: String,
    pub name: String,
    // base64 encoded public key from locker
    pub public_key: String
}

#[allow(dead_code)]
pub struct TenantRetrieveRequest {
    pub tenant_id: String
}

#[allow(dead_code)]
pub struct TenentDeleteResponse {
    pub name: String,
}
