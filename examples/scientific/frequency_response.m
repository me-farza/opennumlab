% Original manufactured frequency-response workload; scientific-v1 profile.
% Full Y is verified from a complete exported binding, never from a preview.
n=128;
rhs=32;
rows=(1:n)';
columns=1:rhs;
Y=complex(zeros([n,rhs,8]),zeros([n,rhs,8]));
for p=1:8
    omega=p./8;
    exact=complex(rows+columns,rows-columns+omega);
    previous=complex(rows+columns-1,rows-columns-1+omega);
    following=complex(rows+columns+1,rows-columns+1+omega);
    previous(1,:)=0;
    following(n,:)=0;
    A=zeros(n,n);
    A(1:n+1:n*n)=complex(4,omega);
    A(2:n+1:n*n)=-1;
    A(n+1:n+1:n*n)=-1;
    B=complex(4,omega).*exact-previous-following;
    X=solve_page(A,B);
    Y(:,:,p)=X;
    residual_norm=norm_inf(B-A*X);
    normalized_residual=residual_norm./(norm_inf(A).*norm_inf(X)+norm_inf(B));
end
final_real=real(Y(1,1,8));
final_imag=imag(Y(1,1,8));
function X=solve_page(A,B)
    X=A\B;
end
