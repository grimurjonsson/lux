Output assets/hero.gif

Set Shell "bash"
Set FontSize 14
Set Width 900
Set Height 500
Set Padding 20
Set Theme "Catppuccin Mocha"

Type "lux demo/sample.log"
Enter
Sleep 3s

Hide
Type "clear"
Enter
Show

Type "lux -r 'ERROR:black+bg-red' demo/sample.log"
Enter
Sleep 3s

Hide
Type "clear"
Enter
Show

Type "lux -t ERROR -b 5 -a 10 demo/sample.log"
Enter
Sleep 3s

Sleep 2s
