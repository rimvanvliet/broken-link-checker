#!/usr/bin/env bash

DATE="$(date +%F)"
HC_HOME=~/hc/hc
DOW=$(date +%u)
mkdir -p $HC_HOME/log

hc >$HC_HOME/log/hc-${DATE}.log

if [ $? -eq 0 ]; then
  if [ $DOW -eq 1 ]; then
    printf "To: stevensbikeservice@gmail.com\nFrom: rimvanvliet@gmail.com\nSubject: Wekelijkse rapportage van de HTTP check op $DATE: GEEN gebroken links\n\n" >$HC_HOME/hc-mail.txt
    cat $HC_HOME/log/hc-${DATE}.log >>$HC_HOME/hc-mail.txt
    ssmtp stevensbikeservice@gmail.com <$HC_HOME/hc-mail.txt
  fi
else
  printf "To: stevensbikeservice@gmail.com\nFrom: rimvanvliet@gmail.com\nSubject: HTTP check van $DATE: *** ER ZIJN GEBROKEN LINKS ***\n\n" >$HC_HOME/hc-mail.txt
  cat $HC_HOME/log/hc-${DATE}.log >>$HC_HOME/hc-mail.txt
  ssmtp stevensbikeservice@gmail.com <$HC_HOME/hc-mail.txt
fi
