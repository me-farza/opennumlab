pf_done=false;
a=Kc01(); b=Kc01(9);
pf_counts=[a.SeenIn a.SeenOut b.SeenIn b.SeenOut];
pf_readings=[a.Reading b.Reading];
pf_bare=bareConstructorK01();
pf_done=true;
function result=bareConstructorK01()
    Kc01(11);
    value=ans;
    result=[value.SeenIn value.SeenOut value.Reading];
end
