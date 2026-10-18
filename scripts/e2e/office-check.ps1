# Open a .pptx in PowerPoint (COM) and, for every embedded OLE object on its slides, print what
# Office and ChemDraw make of it: `slide<TAB>progid<TAB>width_pt<TAB>height_pt<TAB>atoms<TAB>smiles`.
# The atoms and SMILES come from the object loaded by its server (ChemDraw) inside PowerPoint,
# so a line with atoms means Office + ChemDraw open the object as their own.
#
#   pwsh scripts/e2e/office-check.ps1 file.pptx
param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = 'Stop'
$full = (Resolve-Path $Path).Path
$ppt = New-Object -ComObject PowerPoint.Application
try {
    # ReadOnly, Untitled = false, WithWindow = false.
    $pres = $ppt.Presentations.Open($full, -1, 0, 0)
    try {
        foreach ($slide in $pres.Slides) {
            foreach ($shape in $slide.Shapes) {
                if ($shape.Type -ne 7) { continue }  # msoEmbeddedOLEObject
                $prog = $shape.OLEFormat.ProgID
                # As saved, before the object's server is asked for anything.
                $width = [math]::Round($shape.Width, 1)
                $height = [math]::Round($shape.Height, 1)
                $atoms = ''
                $smiles = ''
                try {
                    $object = $shape.OLEFormat.Object
                    $atoms = $object.Atoms.Count
                    $smiles = $object.Objects.Data('chemical/x-smiles')
                } catch {
                    $smiles = "error: $($_.Exception.Message)"
                }
                "{0}`t{1}`t{2}`t{3}`t{4}`t{5}" -f $slide.SlideIndex, $prog, $width, $height, $atoms, $smiles
            }
        }
    } finally {
        $pres.Close()
    }
} finally {
    $ppt.Quit()
}
