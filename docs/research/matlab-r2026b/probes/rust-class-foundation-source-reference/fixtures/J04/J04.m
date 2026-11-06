pf_done=false; pf_mutation_reached=false;
a=Jh04; b=a;
pf_before_valid=[isvalid(a) isvalid(b)];
delete(a);
pf_after_valid=[isvalid(a) isvalid(b)];
pf_mutation_reached=true;
b.Reading=9;
pf_mutation_completed=true;
pf_done=true;
