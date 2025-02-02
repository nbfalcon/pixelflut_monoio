import socket
import random
from PIL import Image

HOST = "127.0.0.1"
PORT = 4000
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.connect((HOST, PORT))
send = sock.send


def pixel(x, y, r, g, b, a=255):
    if a == 255:
        send(b"PX %d %d %02x%02x%02x\n" % (x, y, r, g, b))
    else:
        send(b"PX %d %d %02x%02x%02x%02x\n" % (x, y, r, g, b, a))


def rect(x, y, w, h, r, g, b):
    for i in range(x, x + w):
        for j in range(y, y + h):
            pixel(i, j, r, g, b)



def worm(x, y, n, r, g, b):
    while n:
        pixel(x, y, r, g, b, 25)
        x += random.randint(0, 2) - 1
        y += random.randint(0, 2) - 1
        n -= 1

rect(0, 0, 128, 128, 0xFF, 0xFF, 0);