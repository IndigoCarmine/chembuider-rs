# Have ChemDraw copy a drawing: open the CDX file in ChemDraw (COM), Edit > Select All, Edit > Copy,
# as a user copying a structure to paste elsewhere. Prints the clipboard's formats afterwards.
#
#   pwsh -STA scripts/e2e/chemdraw-copy.ps1 file.cdx
param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = 'Stop'
$full = (Resolve-Path $Path).Path
$app = New-Object -ComObject ChemDraw_x64.Application
try {
    $app.Visible = $true
    $doc = $app.Documents.Open($full)
    $doc.Activate()
    $edit = $null
    $menus = $app.MenuBars.Item(1).Menus
    for ($i = 1; $i -le $menus.Count; $i++) {
        if ($menus.Item($i).caption -eq '&Edit') { $edit = $menus.Item($i); break }
    }
    $items = @{}
    for ($j = 1; $j -le $edit.MenuItems.Count; $j++) {
        $item = $edit.MenuItems.Item($j)
        $items[$item.caption] = $item
    }
    $items['Select All'].Execute()
    $items['Copy'].Execute()
    Start-Sleep -Milliseconds 500
    $doc.Close($false)
} finally {
    $app.Quit()
}
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Clipboard]::GetDataObject().GetFormats() -join ', '
