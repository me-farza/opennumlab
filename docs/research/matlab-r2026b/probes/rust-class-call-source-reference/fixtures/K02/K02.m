pf_done=false;
a=Km02(4);
[b,counts]=a.withReading(9);
pf_two=[a.Reading b.Reading counts];
c=a.withReading(12);
pf_one=[a.Reading c.Reading c.SeenIn c.SeenOut];
pf_bare=bareMethodK02(a);
pf_done=true;
function result=bareMethodK02(value)
    value.withReading(17);
    changed=ans;
    result=[changed.Reading changed.SeenIn changed.SeenOut];
end
