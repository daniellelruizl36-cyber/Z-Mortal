$p = 'C:\Users\Dream\AppData\Local\Temp\claude\C--Users-Dream-Desktop-Z-Mortal\813ee6b7-3bf9-43aa-9cb6-87baec7ca430\tasks\btcbrxgda.output'
$fs = [IO.File]::Open($p, 'Open', 'Read', 'ReadWrite')
$t = (New-Object IO.StreamReader($fs, [Text.Encoding]::BigEndianUnicode)).ReadToEnd()
$lines = $t -split "`r`n|`n"
$out = @()
$out += $lines | Select-String -Pattern 'Running Riichi|Generated|error:|Error|Compiling|R4DEMO|Starting AI' | Select-Object -Last 12 | ForEach-Object { $_.Line }
$out += '---TAIL---'
$out += $lines | Where-Object { $_ -ne '' } | Select-Object -Last 6
[IO.File]::WriteAllLines('C:\Users\Dream\Desktop\Z-Mortal\ra_bridge\log_tail.txt', $out, [Text.UTF8Encoding]::new($false))
