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
$asm = [Reflection.Assembly]::LoadFrom((Resolve-Path -LiteralPath $Dll).Path)
$xmlMeta = $asm.GetType('CodeWalker.GameFiles.XmlMeta')
$metaXml = $asm.GetType('CodeWalker.GameFiles.MetaXml')
$fmtType = $asm.GetType('CodeWalker.GameFiles.MetaFormat')
$rpfFile = $asm.GetType('CodeWalker.GameFiles.RpfFile')

if ($ToBinary) {
    foreach ($f in (Get-ChildItem -LiteralPath $ToBinary -Filter '*.xml' -File)) {
        $name = $f.Name.ToLower()
        $fmt = $null
        if ($name.EndsWith('.ymap.xml') -or $name.EndsWith('.ytyp.xml')) { $fmt = 'RSC' }
        elseif ($name.EndsWith('.ymf.xml') -or $name.EndsWith('.ymt.xml')) { $fmt = 'PSO' }
        elseif ($name.EndsWith('.rel.xml')) { $fmt = 'AudioRel' }
        elseif ($name.EndsWith('.ydr.xml')) { $fmt = 'Ydr' }
        elseif ($name.EndsWith('.ydd.xml')) { $fmt = 'Ydd' }
        elseif ($name.EndsWith('.yft.xml')) { $fmt = 'Yft' }
        elseif ($name.EndsWith('.ytd.xml')) { $fmt = 'Ytd' }
        elseif ($name.EndsWith('.ybn.xml')) { $fmt = 'Ybn' }
        elseif ($name.EndsWith('.ynd.xml')) { $fmt = 'Ynd' }
        if (-not $fmt) { continue }
        if ($fmt -eq 'PSO') {
            # manifeste de dépendances : comme EditProjectManifestPanel.SaveManifestButton_Click (0x06001196),
            # XmlPso.GetPso(document) puis PsoFile.Save()
            try {
                $doc = [System.Xml.XmlDocument]::new()
                $doc.Load($f.FullName)
                $p = New-Object 'object[]' 1
                $p[0] = $doc.psobject.BaseObject
                $pso = $asm.GetType('CodeWalker.GameFiles.XmlPso').GetMethod('GetPso', [type[]]@([System.Xml.XmlDocument])).Invoke($null, $p)
                $data = [byte[]]$pso.Save()
                $outPath = Join-Path $f.DirectoryName ($f.Name.Substring(0, $f.Name.Length - 4))
                [IO.File]::WriteAllBytes($outPath, $data)
                Write-Output ("OK`t" + $f.Name + "`t" + $data.Length)
            } catch {
                Write-Output ("ERREUR`t" + $f.Name + "`t" + $_.Exception.GetBaseException().Message)
            }
            continue
        }
        if ($fmt -eq 'Ytd') {
            # Dictionnaire de textures : construit comme la map désert (build_ytd_dxt.ps1, validée en jeu) :
            # DDSIO.GetTexture sur chaque .dds, TextureDictionary.BuildFromTextureList (0x06005984), YtdFile.Save.
            # Accepte les deux formes de XML (Item directement sous la racine, ce que lit
            # TextureDictionary.ReadXml 0x0600597e, ou sous <Textures>, qui rendait un fichier vide de 57 octets).
            try {
                $doc = [System.Xml.XmlDocument]::new()
                $doc.Load($f.FullName)
                $folder = Join-Path $f.DirectoryName ($f.Name.Substring(0, $f.Name.IndexOf('.')))
                if (-not (Test-Path -LiteralPath $folder)) { $folder = $f.DirectoryName }
                $texType = $asm.GetType('CodeWalker.GameFiles.Texture')
                $listType = [System.Collections.Generic.List``1].MakeGenericType($texType)
                $list = [Activator]::CreateInstance($listType)
                $ddsio = $asm.GetType('CodeWalker.Utils.DDSIO')
                $jenk = $asm.GetType('CodeWalker.GameFiles.JenkHash')
                foreach ($it in $doc.SelectNodes('//Item')) {
                    $fn = $it.SelectSingleNode('FileName')
                    $nm = $it.SelectSingleNode('Name')
                    if ($fn -eq $null -or $nm -eq $null) { continue }
                    $p = New-Object 'object[]' 1
                    $p[0] = [IO.File]::ReadAllBytes((Join-Path $folder $fn.InnerText))
                    $tex = $ddsio.GetMethod('GetTexture').Invoke($null, $p)
                    $tex.Name = $nm.InnerText
                    try { $q = New-Object 'object[]' 1; $q[0] = [string]$nm.InnerText; $tex.NameHash = $jenk.GetMethod('GenHash', [type[]]@([string])).Invoke($null, $q) } catch {}
                    $list.Add($tex)
                }
                $td = [Activator]::CreateInstance($asm.GetType('CodeWalker.GameFiles.TextureDictionary'))
                $td.BuildFromTextureList($list)
                $ytd = [Activator]::CreateInstance($asm.GetType('CodeWalker.GameFiles.YtdFile'))
                $ytd.TextureDict = $td
                $data = [byte[]]$ytd.Save()
                $outPath = Join-Path $f.DirectoryName ($f.Name.Substring(0, $f.Name.Length - 4))
                [IO.File]::WriteAllBytes($outPath, $data)
                Write-Output ("OK`t" + $f.Name + "`t" + $data.Length)
            } catch {
                Write-Output ("ERREUR`t" + $f.Name + "`t" + $_.Exception.GetBaseException().Message)
            }
            continue
        }
        try {
            $doc = [System.Xml.XmlDocument]::new()
            $doc.Load($f.FullName)
            $folder = Join-Path $f.DirectoryName ($f.Name.Substring(0, $f.Name.IndexOf('.')))
            if (-not (Test-Path -LiteralPath $folder)) { $folder = $f.DirectoryName }
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
            # type de fichier selon l'extension (collisions .ybn : MetaXml.GetXml(YbnFile, out filename) -> YbnXml.GetXml)
            $low = $fname.ToLower()
            $tname = if ($low.EndsWith('.ytyp')) { 'CodeWalker.GameFiles.YtypFile' } elseif ($low.EndsWith('.ybn')) { 'CodeWalker.GameFiles.YbnFile' } elseif ($low.EndsWith('.ynd')) { 'CodeWalker.GameFiles.YndFile' } elseif ($low.EndsWith('.ydr')) { 'CodeWalker.GameFiles.YdrFile' } else { 'CodeWalker.GameFiles.YmapFile' }
            $ftype = $asm.GetType($tname)
            $g = $rpfFile.GetMethod('GetResourceFile').MakeGenericMethod($ftype)
            $p = New-Object 'object[]' 1
            $p[0] = $bytes.psobject.BaseObject
            $file = $g.Invoke($null, $p)
            if ($low.EndsWith('.ydr')) {
                # modèle : MetaXml.GetXml(YdrFile, out filename, dossier) écrit aussi ses textures (.dds) dans le dossier
                $m3 = $metaXml.GetMethods() | Where-Object { $_.Name -eq 'GetXml' -and $_.GetParameters().Count -eq 3 -and $_.GetParameters()[0].ParameterType -eq $ftype } | Select-Object -First 1
                $dir = [IO.Path]::Combine([IO.Path]::GetDirectoryName($out), [IO.Path]::GetFileName($out).Split('.')[0])
                [IO.Directory]::CreateDirectory($dir) | Out-Null
                $q = New-Object 'object[]' 3
                $q[0] = $file.psobject.BaseObject
                $q[1] = [string]$fname
                $q[2] = [string]$dir
                $xml = [string]$m3.Invoke($null, $q)
            } else {
                $m2 = $metaXml.GetMethods() | Where-Object { $_.Name -eq 'GetXml' -and $_.GetParameters().Count -eq 2 -and $_.GetParameters()[0].ParameterType -eq $ftype } | Select-Object -First 1
                $q = New-Object 'object[]' 2
                $q[0] = $file.psobject.BaseObject
                $q[1] = [string]$fname
                $xml = [string]$m2.Invoke($null, $q)
            }
            [IO.File]::WriteAllText($out, $xml, (New-Object System.Text.UTF8Encoding($false)))
            Write-Output ("OK`t" + $fname)
        } catch {
            Write-Output ("ERREUR`t" + $inp + "`t" + $_.Exception.GetBaseException().Message)
        }
    }
}
