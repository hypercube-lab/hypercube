import os

directory = os.getcwd()
for filename in os.listdir(directory):
    if filename.endswith(".rs") or filename.endswith(".png"):
        print(os.path.join(directory, filename))

        with open(filename, "a+") as file_object:
                # Move read cursor to the start of file.
                file_object.seek(0)
                # If file is not empty then append '\n'
                data = file_object.read(100)
                if len(data) > 0 :
                    file_object.write("\n")
                # Append text at the end of file
                #file_object.write("hello hi")
                
    else:
        continue