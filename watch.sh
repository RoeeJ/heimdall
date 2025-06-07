#!/bin/bash
watch -n 1 dig google.com @127.0.0.1 -p 1053 +time=1 +tries=1
