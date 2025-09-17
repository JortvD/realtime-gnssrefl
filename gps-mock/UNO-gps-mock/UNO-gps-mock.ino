/*
  Arduino UNO NMEA Sentence Sender
  Repeats a set of GPS NMEA sentences over UART at 115200 baud.
*/

#include <avr/pgmspace.h>

const char nmea0[] PROGMEM = "$GNRMC,082444.000,A,5159.913540,N,00422.397118,E,0.38,309.34,160925,,,D,V*0D";
const char nmea1[] PROGMEM = "$GNVTG,309.34,T,,M,0.38,N,0.70,K,D*27";
const char nmea2[] PROGMEM = "$GNGGA,082444.000,5159.913540,N,00422.397118,E,2,24,1.16,32.424,M,47.111,M,,*4D";
const char nmea3[] PROGMEM = "$GNGSA,A,3,19,01,17,06,09,04,,,,,,,1.48,1.16,0.92,1*05";
const char nmea4[] PROGMEM = "$GNGSA,A,3,66,81,88,87,,,,,,,,,1.48,1.16,0.92,2*04";
const char nmea5[] PROGMEM = "$GNGSA,A,3,15,08,,,,,,,,,,,1.48,1.16,0.92,3*0F";
const char nmea6[] PROGMEM = "$GNGSA,A,3,19,35,29,,,,,,,,,,1.48,1.16,0.92,4*01";
const char nmea7[] PROGMEM = "$GPGSV,4,1,13,03,66,071,,19,41,269,19,01,38,138,20,17,37,239,16,1*61";
const char nmea8[] PROGMEM = "$GPGSV,4,2,13,06,36,305,15,09,33,208,36,34,29,191,33,04,67,164,21,1*67";
const char nmea9[] PROGMEM = "$GPGSV,4,3,13,31,27,068,,28,17,038,,02,10,141,15,12,04,334,,1*6F";
const char nmea10[] PROGMEM = "$GPGSV,4,4,13,25,02,001,,1*52";
const char nmea11[] PROGMEM = "$GPGSV,1,1,04,01,38,138,25,04,67,164,26,06,36,305,17,09,33,208,21,8*6F";
const char nmea12[] PROGMEM = "$GLGSV,2,1,07,66,27,231,26,72,40,045,,81,37,321,17,88,74,213,18,1*7C";
const char nmea13[] PROGMEM = "$GLGSV,2,2,07,87,17,157,27,73,14,026,,80,04,335,,1*4C";
const char nmea14[] PROGMEM = "$GAGSV,3,1,09,26,05,306,,15,66,140,18,03,66,098,,13,55,300,18,7*7E";
const char nmea15[] PROGMEM = "$GAGSV,3,2,09,08,41,192,28,05,23,047,,34,15,130,15,23,07,321,11,7*72";
const char nmea16[] PROGMEM = "$GAGSV,3,3,09,31,03,007,,7*4C";
const char nmea17[] PROGMEM = "$GAGSV,1,1,02,08,41,192,22,15,66,140,18,1*78";
const char nmea18[] PROGMEM = "$GBGSV,4,1,15,48,,,17,19,56,237,18,35,52,279,19,29,38,186,26,1*47";
const char nmea19[] PROGMEM = "$GBGSV,4,2,15,09,30,076,,06,29,062,,16,27,052,,44,17,318,14,1*7A";
const char nmea20[] PROGMEM = "$GBGSV,4,3,15,05,15,118,,32,14,061,,23,10,046,,37,06,092,,1*74";
const char nmea21[] PROGMEM = "$GBGSV,4,4,15,22,05,240,18,25,01,002,,20,70,065,,1*4A";
const char nmea22[] PROGMEM = "$GBGSV,1,1,04,19,56,237,15,35,52,279,20,29,38,186,25,48,,,23,5*45";
const char nmea23[] PROGMEM = "$GNGLL,5159.913540,N,00422.397118,E,082444.000,A,D*4F";

// Table of pointers to the sentence strings
const char* const nmeaSentences[] PROGMEM = {
  nmea0,nmea1,nmea2,nmea3,nmea4,nmea5,nmea6,nmea7,nmea8,nmea9,
  nmea10,nmea11,nmea12,nmea13,nmea14,nmea15,nmea16,nmea17,
  nmea18,nmea19,nmea20,nmea21,nmea22,nmea23
};

const int numSentences = sizeof(nmeaSentences) / sizeof(nmeaSentences[0]);

char buffer[120];  // temp RAM buffer to copy each sentence

void setup() {
  Serial.begin(115200);
}

void loop() {
  for (int i = 0; i < numSentences; i++) {
    strcpy_P(buffer, (char*)pgm_read_word(&(nmeaSentences[i])));
    Serial.println(buffer);
  }
  delay(1000);   // delay before repeating cycle
}
