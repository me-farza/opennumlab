classdef Kc01
    properties
        Reading = 4
        SeenIn = -1
        SeenOut = -1
    end
    methods
        function obj=Kc01(reading)
            obj.SeenIn=nargin;
            obj.SeenOut=nargout;
            if nargin>0
                obj.Reading=reading;
            end
        end
    end
end
