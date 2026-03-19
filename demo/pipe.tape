Output assets/pipe.gif

Set Shell "bash"
Set FontSize 14
Set Width 900
Set Height 500
Set Padding 20
Set Theme "Catppuccin Mocha"

Type "# Pipe any command output through lux"
Enter
Type "cat demo/sample.log | lux"
Enter
Sleep 3s

Hide
Type "clear"
Enter
Show

Type "# Pipe with custom rules"
Enter
Type "cat demo/sample.log | lux -r 'ERROR:bold+red' -r 'WARN:yellow'"
Enter
Sleep 3s

Sleep 2s
