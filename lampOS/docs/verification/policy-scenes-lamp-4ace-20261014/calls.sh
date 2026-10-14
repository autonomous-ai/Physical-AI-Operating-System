#!/bin/zsh
# Busy-room scenes with many calls, for labelled rows (task 17): two people talk, one calls the lamp by name, asks a
# follow-up without it, the two talk on right after the lamp answered, then the other calls it with a bare "Lamp,"
# and follows up. Two scene variants alternate. Prints for each played line whether the lamp spoke after it.
# PLAYS SOUND IN THE ROOM. About 70 s a scene. argv: SCENES LABEL.
cd ~/lamp-voice-data/tools
N=$1; L=$2
for i in $(seq 1 $N); do
  if (( i % 2 )); then
    ./scenario.py $L-$i "Samantha|Did you see the email from the landlord?|0.7" "Daniel|Yes, the rent goes up next month.|0.8" "Samantha|Hey Lamp, how many days are left in October?|10" "Samantha|And what about November?|9" "Daniel|We should look at other places then.|0.7" "Samantha|Maybe, but moving is so much work.|3" "Daniel|Lamp, what is a good way to save money on groceries?|12" "Daniel|Can you give me one more tip?|10" > /dev/null 2>&1
  else
    ./scenario.py $L-$i "Daniel|Are you going to the gym tonight?|0.7" "Samantha|Probably not, I am too tired.|0.8" "Daniel|Hey Lamp, can you suggest a short stretch I can do at my desk?|12" "Daniel|How long should I hold it?|9" "Samantha|I should do that too, my back hurts.|0.7" "Daniel|We sit too much in this office.|3" "Samantha|Lamp, what is a quick healthy snack?|12" "Samantha|Is that better than a banana?|10" > /dev/null 2>&1
  fi
  F=$(ls -t ~/lamp-voice-data/scenarios/*.txt | head -1)
  echo "scene $i:"
  LC_ALL=en_US.UTF-8 /usr/bin/awk '/^#/{next} /PLAY/{ if (p != "") printf "  %-62s | %s\n", p, (said == "" ? "-" : "SPOKE"); p=substr($0, index($0,"PLAY")+6, 62); said=""; next } / LAMP /{ said="y" } END { printf "  %-62s | %s\n", p, (said == "" ? "-" : "SPOKE") }' $F
done
