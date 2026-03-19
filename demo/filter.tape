Output assets/filter.gif

Set Shell "bash"
Set FontSize 14
Set Width 900
Set Height 500
Set Padding 20
Set Theme "Catppuccin Mocha"

Type "# Include only errors and warnings"
Enter
Type "lux -i 'ERROR|WARN' demo/sample.log"
Enter
Sleep 2s

Hide
Type "clear"
Enter
Show

Type "# Exclude noisy debug/trace lines"
Enter
Type "lux -e 'DEBUG' -e 'TRACE' demo/sample.log"
Enter
Sleep 2s

Hide
Type "clear"
Enter
Show

Type "# Combine: api lines only, skip health checks"
Enter
Type "lux -i 'api' -e 'health' demo/sample.log"
Enter
Sleep 2s

Sleep 2s
