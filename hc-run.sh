#!/usr/bin/env bash

DATE="$(date +%F)"
HC_HOME=~/hc/hc
mkdir -p $HC_HOME/log

printf "To: ruud@vliet.io,stevensbikeservice@gmail.com\nFrom: rimvanvliet@gmail.com\nSubject: HTTP check van $DATE\n\n" > $HC_HOME/log/hc-${DATE}.log

hc >> $HC_HOME/log/hc-${DATE}.log

ssmtp ruud@vliet.io < $HC_HOME/log/hc-${DATE}.log

