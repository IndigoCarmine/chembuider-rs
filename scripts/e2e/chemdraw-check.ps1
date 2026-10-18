# Open CDX files in the installed ChemDraw (COM) and print what ChemDraw reads from each:
# atom and bond counts and its SMILES, one line per file as `path<TAB>atoms<TAB>bonds<TAB>smiles`.
# With -Cdxml, ChemDraw also saves each as .cdxml beside it (to compare the details).
#
#   pwsh scripts/e2e/chemdraw-check.ps1 a.cdx b.cdx [-Cdxml]
param(
    [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)][string[]]$Paths,
    [switch]$Cdxml
)
$ErrorActionPreference = 'Stop'
$app = New-Object -ComObject ChemDraw_x64.Application
try {
    foreach ($path in $Paths) {
        $full = (Resolve-Path $path).Path
        $doc = $app.Documents.Open($full)
        try {
            $smiles = ''
            if ($doc.Atoms.Count -gt 0) { $smiles = $doc.Objects.Data('chemical/x-smiles') }
            "{0}`t{1}`t{2}`t{3}" -f $full, $doc.Atoms.Count, $doc.Bonds.Count, $smiles
            if ($Cdxml) { $doc.SaveAs([IO.Path]::ChangeExtension($full, '.cdxml')) }
        } finally {
            $doc.Close($false)
        }
    }
} finally {
    $app.Quit()
}
