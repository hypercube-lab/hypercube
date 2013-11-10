import os

directory = os.getcwd()
for filename in os.listdir(directory):
    if filename.endswith(".rs") or filename.endswith(".png"):
        print(os.path.join(directory, filename))

        #remove last line from a text line in python
        fd=open(filename,"r")
        d=fd.read()
        fd.close()
        m=d.split("\n")
        s="\n".join(m[:-1])
        fd=open(filename,"w+")
        for i in range(len(s)):
            fd.write(s[i])
        fd.close()
                
    else:
        continue