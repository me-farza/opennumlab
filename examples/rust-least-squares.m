% Original headless class-profile noisy least-squares workload, contract01.
% Entire output is exported as one lossless complex column vector for an independent oracle.
ls_C=complex(zeros(32,32),zeros(32,32));
for ls_j=1:32
    if ls_j<=8
        ls_C(ls_j,ls_j)=1;
    elseif ls_j<=16
        ls_C(ls_j,ls_j)=complex(0,2);
    elseif ls_j<=24
        ls_C(ls_j,ls_j)=-4;
    else
        ls_C(ls_j,ls_j)=complex(0,-8);
    end
end
ls_r=(1:32)';
ls_c=1:8;
ls_Xstar=complex((ls_r+2*ls_c)/64,(2*ls_r-ls_c)/128);
ls_A=ls_C;
ls_B=ls_C*ls_Xstar;
for ls_stage=1:7
    ls_A=[ls_A;ls_A];
    ls_B=[ls_B;ls_B];
end
ls_weights=[1:127,-8128];
ls_weight_column=reshape(ones(32,1)*ls_weights,[4096,1]);
ls_noise_row=complex(ones(1,8),ls_c/16);
ls_noise=(ls_weight_column*ls_noise_row)/16;
ls_B=ls_B+ls_noise;
ls_X=ls_A\ls_B;
ls_R=ls_A*ls_X-ls_B;
ls_validation=[ls_A(:);ls_B(:);ls_X(:);ls_R(:)];
ls_done=true;
