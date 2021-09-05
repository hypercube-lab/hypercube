import os

def loopfile():
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

def commit():
    os.system('git commit -a -m "merge and update" > /dev/null 2>&1')

def set_sys_time(year, month, day):
    os.system('date -s %04d%02d%02d' % (year, month, day))

if __name__ == '__main__':
    set_sys_time(2017, 1, 1)
    loopfile()
    commit()