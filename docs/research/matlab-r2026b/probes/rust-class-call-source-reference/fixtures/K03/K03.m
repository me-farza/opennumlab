pf_done=false;
a=Ks03(3);
pf_constructed=a.Reading;
[pf_class,pf_reading,pf_object]=localShadowK03();
pf_done=true;
function [kind,reading,object]=localShadowK03()
    Ks03=[7 8];
    selected=Ks03(2);
    kind=class(selected);
    object=isobject(selected);
    if object
        reading=selected.Reading;
    else
        reading=selected;
    end
end
