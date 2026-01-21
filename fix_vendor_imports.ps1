$base = "C:\Users\isich\braid_axum_http\1_1\implementations\braid_rs"
$dt_src = "C:\Users\isich\braid_axum_http\reference\diamond_types\src"
$rle_src = "C:\Users\isich\braid_axum_http\reference\diamond_types\crates\rle\src"
$dt_dest = "$base\src\vendor\diamond_types"
$rle_dest = "$base\src\vendor\rle"

Write-Host "Resetting sources..."
if (Test-Path $dt_dest) { Remove-Item -Path $dt_dest -Recurse -Force }
if (Test-Path $rle_dest) { Remove-Item -Path $rle_dest -Recurse -Force }
New-Item -ItemType Directory -Path $dt_dest -Force | Out-Null
New-Item -ItemType Directory -Path $rle_dest -Force | Out-Null

Copy-Item -Path "$dt_src\*" -Destination $dt_dest -Recurse -Force
Copy-Item -Path "$rle_src\*" -Destination $rle_dest -Recurse -Force

if (Test-Path "$dt_dest\lib.rs") { Move-Item -Path "$dt_dest\lib.rs" -Destination "$dt_dest\mod.rs" -Force }
if (Test-Path "$rle_dest\lib.rs") { Move-Item -Path "$rle_dest\lib.rs" -Destination "$rle_dest\mod.rs" -Force }

function Fix-File($path, $is_rle) {
    # Read as UTF-8
    $content = [System.IO.File]::ReadAllText($path)
    
    if ($is_rle) {
        # rle-internal crate:: -> crate::vendor::rle::
        # Case-insensitive is fine for 'crate::'
        $content = $content -replace 'crate::', 'crate::vendor::rle::'
    }
    else {
        # 1. Protect EVERYTHING first using CASE-SENSITIVE replacement where needed
        
        # Internal rle module in DT
        $content = $content -creplace 'crate::rle::', 'INTERNAL_RLE_MOD::'
        
        # crate:: -> DT_INT_ROOT::
        $content = $content -creplace 'crate::', 'DT_INT_ROOT::'
        
        # External rle crate (case sensitive to avoiding matching Rle struct if any)
        $content = $content -creplace '::rle::', 'RLE_EXT_CRATE::'
        $content = $content -creplace '(?<!::)rle::', 'RLE_EXT_CRATE::'
        
        # use list:: style (MUST be case sensitive to avoid matching List::)
        $internal_mods = @("list", "causalgraph", "frontier", "dtrange", "unicount", "rev_range", "check", "encoding", "wal", "ost", "listmerge", "listmerge2", "textinfo", "storage", "simple_checkout", "stats", "branch", "oplog")
        foreach ($mod in $internal_mods) {
            # Use -creplace for case sensitivity!
            $content = $content -creplace "(?m)^use $mod`::", "use DT_INT_ROOT::$mod`::"
            $content = $content -creplace " $mod`::", " DT_INT_ROOT::$mod`::"
        }
        
        # 2. Resolve placeholders
        $content = $content.Replace('INTERNAL_RLE_MOD::', 'crate::vendor::diamond_types::rle::')
        $content = $content.Replace('DT_INT_ROOT::', 'crate::vendor::diamond_types::')
        $content = $content.Replace('RLE_EXT_CRATE::', 'crate::vendor::rle::')
    }
    
    # 3. Final cleanup
    # Handle accidental double-wraps if they somehow occurred
    $content = $content.Replace('crate::vendor::diamond_types::vendor::diamond_types::', 'crate::vendor::diamond_types::')
    $content = $content.Replace('crate::vendor::diamond_types::crate::vendor::rle::', 'crate::vendor::rle::')
    
    [System.IO.File]::WriteAllText($path, $content)
}

Write-Host "Applying case-sensitive fixes..."
Get-ChildItem $dt_dest -Recurse -Filter *.rs | ForEach-Object { Fix-File $_.FullName $false }
Get-ChildItem $rle_dest -Recurse -Filter *.rs | ForEach-Object { Fix-File $_.FullName $true }

Write-Host "Done!"
