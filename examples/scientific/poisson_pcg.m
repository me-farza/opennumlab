% Generic triplet assembly and a user-function Jacobi-PCG; no domain builtin.
n=32; N=n*n*n; M=(n-1)*n*n;
I=reshape(1:N,n,n,n); p=(1:N)';
a=reshape(I(1:n-1,:,:),M,1); b=a+1;
c=reshape(I(:,1:n-1,:),M,1); d=c+n;
e=reshape(I(:,:,1:n-1),M,1); f=e+n*n;
rows=[p;a;b;c;d;e;f]; cols=[p;b;a;d;c;f;e];
values=[6*ones(N,1);-ones(6*M,1)];
A=sparse(rows,cols,values,N,N);
t=1:n; q=t.*(n+1-t);
gx=reshape(q,n,1,1); gy=reshape(q,1,n,1); gz=reshape(q,1,1,n);
rhs=reshape(2.*(gy.*gz+gx.*gz+gx.*gy),N,1);
[solution,flag,iterations,true_residual]=jacobi_pcg(A,rhs,6,512,2^-30);

function [x,flag,iterations,true_residual]=jacobi_pcg(A,b,diagonal,maxit,tolerance)
    x=zeros(numel(b),1); flag=1; iterations=0;
    r=b; scale=norm_inf(b); true_residual=scale;
    if diagonal<=0
        flag=2; return;
    end
    if scale==0
        flag=0; true_residual=0; return;
    end
    z=r/diagonal; p=z; rho=r'*z;
    while iterations<maxit
        Ap=A*p; denominator=p'*Ap;
        if denominator<=0
            flag=2; break;
        end
        alpha=rho/denominator;
        x=x+alpha*p; r=r-alpha*Ap; iterations=iterations+1;
        if norm_inf(r)<=tolerance*scale
            true_r=b-A*x; true_residual=norm_inf(true_r);
            if true_residual<=tolerance*scale
                flag=0; break;
            end
            r=true_r;
        end
        z=r/diagonal; rho_new=r'*z;
        if rho_new<=0
            flag=2; break;
        end
        beta=rho_new/rho; p=z+beta*p; rho=rho_new;
    end
    true_residual=norm_inf(b-A*x);
    if true_residual<=tolerance*scale
        flag=0;
    elseif flag==0
        flag=1;
    end
end
