install libcamera-dev 
        libclang1-19
        libcamera0.3
        clang

rm /usr/lib/aarch64-linux-gnu/libcamera.so.0.4
rm /usr/lib/aarch64-linux-gnu/libcamera.so.0.4.0
# ldconfig -X -v

ls -la | grep libcamera
cd /usr/lib/aarch64-linux-gnu/
sudo ln -s libcamera-base.so.0.3.2 libcamera-base.so.0.3
sudo ln -s libcamera.so.0.3 libcamera.so.0.3.2
sudo aptitude install libpisp1