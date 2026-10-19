# Downloads both MSIs of the newest published release that shipped one for this machine, for CI and the release, and
# prints its tag, or nothing where no release has. gh reads GH_TOKEN, and GITHUB_REPOSITORY names the repository.
param(
    [Parameter(Mandatory)] [ValidateSet('x64', 'arm64')] [string] $Installer,
    [Parameter(Mandatory)] [string] $Destination,
    # The release being made, never its own earlier one.
    [string] $Except
)
$ErrorActionPreference = 'Stop'

$releases = gh api --paginate "repos/$env:GITHUB_REPOSITORY/releases?per_page=100" --jq '.[] | select(.draft == false) | [.tag_name, .assets[].name] | join(" ")'
if ($LASTEXITCODE -ne 0) { throw "gh api exited $LASTEXITCODE" }
foreach ($release in $releases) {
    $tag, $assets = $release -split ' '
    if ($tag -ne $Except -and $assets -contains "Pitboard-$tag-windows-$Installer.msi") {
        gh release download $tag --repo $env:GITHUB_REPOSITORY --pattern "Pitboard-$tag-windows-*.msi" --dir $Destination | Out-Host
        if ($LASTEXITCODE -ne 0) { throw "gh release download exited $LASTEXITCODE" }
        return $tag
    }
}
