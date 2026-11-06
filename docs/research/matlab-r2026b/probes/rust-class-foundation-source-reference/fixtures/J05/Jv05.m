classdef Jv05
    properties
        Reading = 4
    end
    methods
        function obj=Jv05(reading)
            if nargin>0
                obj.Reading=reading;
            end
        end
        function obj=withReading(obj,reading)
            obj.Reading=reading;
        end
    end
end
