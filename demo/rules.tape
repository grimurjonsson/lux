Output assets/rules.gif

Set Shell "bash"
Set FontSize 14
Set Width 900
Set Height 500
Set Padding 20
Set Theme "Catppuccin Mocha"

Hide
Type "export PATH=demo/bin:$PATH"
Enter
Show

Type "# Line scope (default) — color the whole line"
Enter
Type "lux -r 'ERROR:bold+red' -r 'WARN:yellow' demo/sample.log"
Enter
Sleep 2s

Hide
Type "clear"
Enter
Show

Type "# Match scope — color only the matched text"
Enter
Type "lux -r 'ERROR:bold+red:match' -r 'WARN:yellow:match' -r 'INFO:green:match' demo/sample.log"
Enter
Sleep 2s

Hide
Type "clear"
Enter
Show

Type "# Capture group — color a specific part"
Enter
Type "lux -r '\[(\w+)\]:bold+cyan:cap1' -r '(\d+)ms:bold+magenta:cap1' demo/sample.log"
Enter
Sleep 2s

Sleep 2s
