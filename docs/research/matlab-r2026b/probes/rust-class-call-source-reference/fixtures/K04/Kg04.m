classdef Kg04 < handle
    properties
        Reading = 4
        Serial
    end
    methods
        function obj=Kg04()
            persistent serial
            if isempty(serial)
                serial=0;
            end
            serial=serial+1;
            obj.Serial=serial;
        end
    end
end
