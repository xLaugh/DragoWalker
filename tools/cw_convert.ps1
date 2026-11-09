# DragoWalker : conversion XML <-> fichiers binaires du jeu avec le CodeWalker installé sur ce PC.
#   cw_convert.ps1 -Dll <CodeWalker.Core.dll> -ToBinary <dossier>   : *.ymap.xml, *.ytyp.xml, *.ydr.xml... -> binaires
#   cw_convert.ps1 -Dll <CodeWalker.Core.dll> -ToXmlList <liste.txt> : chaque ligne « binaire|sortie.xml »
# Fonctions de CodeWalker utilisées : XmlMeta.GetData (0x06003faf), RpfFile.GetResourceFile<T> (0x06005aa8), MetaXml.GetXml (0x06003d6d / 0x06003d6e).
param(
    [Parameter(Mandatory = $true)][string]$Dll,
    [string]$ToBinary,
    [string]$ToXmlList
)
$ErrorActionPreference = 'Stop'
$asm = [Reflection.Assembly]::LoadFrom((Resolve-Path $Dll).Path)
$xmlMeta = $asm.GetType('CodeWalker.GameFiles.XmlMeta')
$metaXml = $asm.GetType('CodeWalker.GameFiles.MetaXml')
$fmtType = $asm.GetType('CodeWalker.GameFiles.MetaFormat')
$rpfFile = $asm.GetType('CodeWalker.GameFiles.RpfFile')

if ($ToBinary) {
    foreach ($f in (Get-ChildItem -Path $ToBinary -Filter '*.xml' -File)) {
        $name = $f.Name.ToLower()
        $fmt = $null
        if ($name.EndsWith('.ymap.xml') -or $name.EndsWith('.ytyp.xml')) { $fmt = 'RSC' }
        elseif ($name.EndsWith('.ydr.xml')) { $fmt = 'Ydr' }
        elseif ($name.EndsWith('.ydd.xml')) { $fmt = 'Ydd' }
        elseif ($name.EndsWith('.yft.xml')) { $fmt = 'Yft' }
        elseif ($name.EndsWith('.ytd.xml')) { $fmt = 'Ytd' }
        if (-not $fmt) { continue }
        try {
            $doc = [System.Xml.XmlDocument]::new()
            $doc.Load($f.FullName)
            $folder = Join-Path $f.DirectoryName ($f.Name.Substring(0, $f.Name.IndexOf('.')))
            if (-not (Test-Path $folder)) { $folder = $f.DirectoryName }
            $p = New-Object 'object[]' 3
            $p[0] = $doc.psobject.BaseObject
            $p[1] = [Enum]::Parse($fmtType, $fmt)
            $p[2] = [string]$folder
            $data = [byte[]]$xmlMeta.GetMethod('GetData').Invoke($null, $p)
            if ($data -eq $null -or $data.Length -eq 0) { Write-Output ("ERREUR`t" + $f.Name + "`tconversion vide"); continue }
            $outPath = Join-Path $f.DirectoryName ($f.Name.Substring(0, $f.Name.Length - 4))
            [IO.File]::WriteAllBytes($outPath, $data)
            Write-Output ("OK`t" + $f.Name + "`t" + $data.Length)
        } catch {
            Write-Output ("ERREUR`t" + $f.Name + "`t" + $_.Exception.GetBaseException().Message)
        }
    }
}

if ($ToXmlList) {
    foreach ($line in [IO.File]::ReadAllLines($ToXmlList)) {
        if (-not $line.Contains('|')) { continue }
        $parts = $line.Split('|')
        $inp = $parts[0]; $out = $parts[1]
        try {
            $bytes = [IO.File]::ReadAllBytes($inp)
            $fname = [IO.Path]::GetFileName($inp)
            $tname = if ($fname.ToLower().EndsWith('.ytyp')) { 'CodeWalker.GameFiles.YtypFile' } else { 'CodeWalker.GameFiles.YmapFile' }
            $ftype = $asm.GetType($tname)
            $g = $rpfFile.GetMethod('GetResourceFile').MakeGenericMethod($ftype)
            $p = New-Object 'object[]' 1
            $p[0] = $bytes.psobject.BaseObject
            $file = $g.Invoke($null, $p)
            $m2 = $metaXml.GetMethods() | Where-Object { $_.Name -eq 'GetXml' -and $_.GetParameters().Count -eq 2 -and $_.GetParameters()[0].ParameterType -eq $ftype } | Select-Object -First 1
            $q = New-Object 'object[]' 2
            $q[0] = $file.psobject.BaseObject
            $q[1] = [string]$fname
            $xml = [string]$m2.Invoke($null, $q)
            [IO.File]::WriteAllText($out, $xml, (New-Object System.Text.UTF8Encoding($false)))
            Write-Output ("OK`t" + $fname)
        } catch {
            Write-Output ("ERREUR`t" + $inp + "`t" + $_.Exception.GetBaseException().Message)
        }
    }
}
