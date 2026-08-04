# TEMPLATE. `{{VERSION}}` and `{{SHA256_MSI}}` are filled by packaging/render.mjs.
#
# THE MSI, NOT THE NSIS INSTALLER — and this is the one decision in the
# directory worth arguing about, so it is written down here and in
# packaging/README.md.
#
# `choco install` runs from an elevated shell, and the population that uses
# Chocolatey at all is disproportionately fleets: Ansible, Puppet, DSC, an
# image build, a `choco install kavka` in somebody's onboarding script. Those
# run as SYSTEM or as an admin account that is nobody's desktop. Tauri's NSIS
# bundle installs PER-USER by default, so under those runners it lands in the
# service account's profile — the package reports success and the person who
# will actually use the machine has no Kavka. That failure is silent, remote,
# and takes a support thread to diagnose.
#
# The WiX MSI installs per-machine, which is what an elevated package manager
# is for, and `msiexec /qn` is as silent as `/S` is. The cost is that a
# non-admin `choco install` now fails outright instead of installing for the
# current user — which is the right way round: a loud refusal beats an install
# in the wrong profile. Someone who wants the per-user build has winget
# (`winget install SahilHirani.Kavka`, whose default scope is the NSIS one) or
# the .exe on the release page.
#
# The checksum is not optional and not a formality: it is the only thing
# standing between a Chocolatey user and a GitHub release asset that changed
# after this package was reviewed.
$ErrorActionPreference = 'Stop'

$packageName = 'kavka'
$version     = '{{VERSION}}'
$url64       = "https://github.com/sahilhirani/kavka/releases/download/v$version/Kavka_${version}_x64_en-US.msi"

$packageArgs = @{
  packageName    = $packageName
  fileType       = 'msi'
  url64bit       = $url64
  checksum64     = '{{SHA256_MSI}}'
  checksumType64 = 'sha256'
  # /qn is silent, /norestart because a desktop Kafka client has no business
  # rebooting a machine mid-provisioning — the 3010 below is how that choice is
  # reported back rather than hidden.
  silentArgs     = '/qn /norestart'
  # 0 succeeded; 3010 and 1641 are "succeeded, wants a restart". Treating those
  # two as failures is the classic MSI packaging bug: the product is installed
  # and the run is marked red.
  validExitCodes = @(0, 3010, 1641)
  softwareName   = 'Kavka*'
}

Install-ChocolateyPackage @packageArgs

# No chocolateyuninstall.ps1 on purpose: the MSI registers a proper uninstall
# entry with its own ProductCode, so Chocolatey's auto-uninstaller finds and
# drives it. A hand-written uninstall script would be a second, worse copy of
# that — and it would need the ProductCode, which does not exist until WiX has
# run (see the winget installer manifest, which has the same TODO for the same
# reason).
