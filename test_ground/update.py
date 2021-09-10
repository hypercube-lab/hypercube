import os
import datetime


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

                for last_line in file_object:
                    pass
                print(filename)
                print(last_line)
                file_object.close()
                # If the file is updated in the last cycle
                if last_line == "#999":
                    remove_999(filename)
                else:
                    append_999(filename)

        else:
            continue


def remove_999(filename):
    print("Remove last line with 999")
    fd = open(filename, "r")
    d = fd.read()
    fd.close()
    m = d.split("\n")
    s = "\n".join(m[:-1])
    fd = open(filename, "w+")
    for i in range(len(s)):
        fd.write(s[i])
    fd.close()


def append_999(filename):
    print("Append last line with 999")
    with open(filename, "a+") as fil:
        fil.seek(0)
        data = fil.read(100)
        if len(data) > 0:
            fil.write("\n//999")
    fil.close()


def commit():
    os.system('git commit -a -m "merge and update" > /dev/null 2>&1')


def set_sys_time(year, month, day):
    os.system('date -s %04d%02d%02d' % (year, month, day))


if __name__ == '__main__':
    # set_sys_time(2017, 1, 1)
    loopfile()
    # commit()
