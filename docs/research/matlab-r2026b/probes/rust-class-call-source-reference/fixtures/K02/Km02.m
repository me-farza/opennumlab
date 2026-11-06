classdef Km02
    properties
        Reading
        SeenIn = -1
        SeenOut = -1
    end
    methods
        function obj=Km02(reading)
            obj.Reading=reading;
        end
        function [obj,counts]=withReading(obj,reading)
            counts=[nargin nargout];
            obj.SeenIn=nargin;
            obj.SeenOut=nargout;
            obj.Reading=reading;
        end
    end
end
