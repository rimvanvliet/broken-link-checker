#!/usr/bin/env bash

DATE="$(date +%F)"
HC_HOME=~/hc/hc
mkdir -p $HC_HOME/log


hc > $HC_HOME/log/hc-${DATE}.log

if [ $? -eq 0 ]
  then
    printf "To: stevensbikeservice@gmail.com\nFrom: rimvanvliet@gmail.com\nSubject: HTTP check van $DATE: GEEN gebroken links\n\n" > $HC_HOME/hc-mail.txt
    cat $HC_HOME/log/hc-${DATE}.log >> $HC_HOME/hc-mail.txt
  else
    printf "To: stevensbikeservice@gmail.com\nFrom: rimvanvliet@gmail.com\nSubject: HTTP check van $DATE: *** ER ZIJN GEBROKEN LINKS ***\n\n" > $HC_HOME/hc-mail.txt
    cat $HC_HOME/log/hc-${DATE}.log >> $HC_HOME/hc-mail.txt
fi
 
ssmtp stevensbikeservice@gmail.com < $HC_HOME/hc-mail.txt
