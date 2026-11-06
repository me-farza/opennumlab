pf_done=false;
g=Jg03; a=Jw03(g); b=a;
pf_before=[a.Gauge.Reading b.Gauge.Reading g.Reading];
b.Gauge.Reading=9;
pf_after=[a.Gauge.Reading b.Gauge.Reading g.Reading];
pf_done=true;
