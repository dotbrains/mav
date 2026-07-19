
function ParseMavWorkspace {
    $metadata = cargo metadata --no-deps --offline | ConvertFrom-Json
    $env:MAV_WORKSPACE = $metadata.workspace_root
    $env:RELEASE_VERSION = $metadata.packages | Where-Object { $_.name -eq "mav" } | Select-Object -ExpandProperty version
}
