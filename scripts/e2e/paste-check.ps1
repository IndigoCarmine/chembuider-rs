# Paste what is on the clipboard into PowerPoint and into ChemDraw (COM), and print what each
# made of it: PowerPoint's shape (an embedded object's ProgID, and ChemDraw's SMILES of it), and
# ChemDraw's own atoms and SMILES.
#
# With -SaveAs, PowerPoint's presentation is saved there (to check with office-check.ps1: an
# object pasted into a presentation without a window only names its class once saved).
#
#   pwsh scripts/e2e/paste-check.ps1 [-SaveAs pasted.pptx]
param([string]$SaveAs)
$ErrorActionPreference = 'Stop'

$ppt = New-Object -ComObject PowerPoint.Application
try {
    $pres = $ppt.Presentations.Add(0)  # no window
    try {
        $slide = $pres.Slides.Add(1, 12)  # ppLayoutBlank
        $shapes = $slide.Shapes.Paste()
        foreach ($shape in $shapes) {
            $smiles = ''
            if ($shape.Type -eq 7) {
                try { $smiles = $shape.OLEFormat.Object.Objects.Data('chemical/x-smiles') } catch { $smiles = "error: $($_.Exception.Message)" }
                "powerpoint`tole`t$($shape.OLEFormat.ProgID)`t$smiles"
            } else {
                "powerpoint`ttype $($shape.Type)"
            }
        }
        if ($SaveAs) { $pres.SaveAs([IO.Path]::GetFullPath($SaveAs)) }
    } finally {
        $pres.Close()
    }
} finally {
    $ppt.Quit()
}

$app = New-Object -ComObject ChemDraw_x64.Application
try {
    $doc = $app.Documents.Add()
    $doc.Paste()
    $smiles = ''
    if ($doc.Atoms.Count -gt 0) { $smiles = $doc.Objects.Data('chemical/x-smiles') }
    "chemdraw`t$($doc.Atoms.Count)`t$smiles"
    $doc.Close($false)
} finally {
    $app.Quit()
}
