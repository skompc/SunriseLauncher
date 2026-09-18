This build process runs everywhere docker-cli will run... 
which is to say Windows, MacOS, and most Linux distros

to build for deck:
    open the terminal and type:
        $docker compose build

        $docker compose run --rm sunrise-build-deck

    once inside:
        #npm install
        #npm run build:deck

    final package will be ./Project Sunrise Launcher

This Launcher will:
    add an icon to the desktop
    install itself to /home/{USER}/project-sunrise-launcher
    launch said install from anywhere so you can move the file to anywhere you please