#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>


#define FOOTER_SIZE 32

static const unsigned char FOOTER_MAGIC[16] =
{
    'S','U','N','R','I','S','E','_',
    'P','A','Y','L','O','A','D','1'
};


static uint64_t read_le64(const unsigned char *p)
{
    uint64_t value = 0;

    for(int i = 0; i < 8; i++)
    {
        value |= ((uint64_t)p[i]) << (i * 8);
    }

    return value;
}


static int mkdir_recursive(const char *path)
{
    char buffer[PATH_MAX];

    if(strlen(path) >= sizeof(buffer))
    {
        return -1;
    }

    strcpy(buffer, path);

    for(char *p = buffer + 1; *p; p++)
    {
        if(*p == '/')
        {
            *p = '\0';

            if(mkdir(buffer, 0755) != 0 && errno != EEXIST)
            {
                return -1;
            }

            *p = '/';
        }
    }

    if(mkdir(buffer, 0755) != 0 && errno != EEXIST)
    {
        return -1;
    }

    return 0;
}


static int copy_bytes(
    int source,
    int destination,
    uint64_t count
)
{
    unsigned char buffer[65536];

    while(count > 0)
    {
        size_t wanted = sizeof(buffer);

        if((uint64_t)wanted > count)
        {
            wanted = (size_t)count;
        }

        ssize_t received = read(source, buffer, wanted);

        if(received <= 0)
        {
            return -1;
        }

        size_t written_total = 0;

        while(written_total < (size_t)received)
        {
            ssize_t written =
                write(
                    destination,
                    buffer + written_total,
                    (size_t)received - written_total
                );

            if(written <= 0)
            {
                return -1;
            }

            written_total += (size_t)written;
        }

        count -= (uint64_t)received;
    }

    return 0;
}


static int extract_payload(
    int self_fd,
    uint64_t payload_offset,
    uint64_t payload_size,
    const char *directory
)
{
    char temporary_template[] =
        "/tmp/project-sunrise-payload-XXXXXX";

    int temporary_fd =
        mkstemp(temporary_template);

    if(temporary_fd < 0)
    {
        perror("Project Sunrise Launcher: mkstemp");
        return -1;
    }

    if(fchmod(temporary_fd, 0600) != 0)
    {
        close(temporary_fd);
        unlink(temporary_template);
        return -1;
    }

    if(lseek(self_fd, (off_t)payload_offset, SEEK_SET) < 0)
    {
        perror("Project Sunrise Launcher: lseek");
        close(temporary_fd);
        unlink(temporary_template);
        return -1;
    }

    if(copy_bytes(self_fd, temporary_fd, payload_size) != 0)
    {
        fprintf(
            stderr,
            "Project Sunrise Launcher: failed to copy payload\n"
        );

        close(temporary_fd);
        unlink(temporary_template);
        return -1;
    }

    close(temporary_fd);

    pid_t child = fork();

    if(child < 0)
    {
        perror("Project Sunrise Launcher: fork");
        unlink(temporary_template);
        return -1;
    }

    if(child == 0)
    {
        execlp(
            "tar",
            "tar",
            "-xzf",
            temporary_template,
            "-C",
            directory,
            (char *)NULL
        );

        perror("Project Sunrise Launcher: tar");
        _exit(127);
    }

    int status = 0;

    if(waitpid(child, &status, 0) < 0)
    {
        perror("Project Sunrise Launcher: waitpid");
        unlink(temporary_template);
        return -1;
    }

    unlink(temporary_template);

    if(!WIFEXITED(status) || WEXITSTATUS(status) != 0)
    {
        fprintf(
            stderr,
            "Project Sunrise Launcher: payload extraction failed\n"
        );

        return -1;
    }

    return 0;
}


static void create_desktop_entry(
    const char *launcher_path,
    const char *install_directory
)
{
    const char *home = getenv("HOME");

    if(!home)
    {
        return;
    }

    char desktop_directory[PATH_MAX];

    snprintf(
        desktop_directory,
        sizeof(desktop_directory),
        "%s/Desktop",
        home
    );

    if(mkdir_recursive(desktop_directory) != 0)
    {
        return;
    }

    char desktop_file[PATH_MAX];

    snprintf(
        desktop_file,
        sizeof(desktop_file),
        "%s/project-sunrise-launcher.desktop",
        desktop_directory
    );

    char icon_path[PATH_MAX];

    snprintf(
        icon_path,
        sizeof(icon_path),
        "%s/icons/128x128.png",
        install_directory
    );

    FILE *file = fopen(desktop_file, "w");

    if(!file)
    {
        return;
    }

    fprintf(file, "[Desktop Entry]\n");
    fprintf(file, "Type=Application\n");
    fprintf(file, "Name=Project Sunrise Launcher\n");
    fprintf(file, "Comment=Installer and launcher for Project Sunrise\n");
    fprintf(file, "Exec=\"%s\"\n", launcher_path);
    fprintf(file, "Icon=%s\n", icon_path);
    fprintf(file, "Terminal=false\n");
    fprintf(file, "Categories=Game;\n");

    fclose(file);

    chmod(desktop_file, 0644);
}


int main(int argc, char **argv)
{
    char self_path[PATH_MAX];

    ssize_t self_length =
        readlink(
            "/proc/self/exe",
            self_path,
            sizeof(self_path) - 1
        );

    if(self_length <= 0)
    {
        fprintf(
            stderr,
            "Project Sunrise Launcher: cannot determine executable path\n"
        );

        return 1;
    }

    self_path[self_length] = '\0';


    int self_fd = open(self_path, O_RDONLY);

    if(self_fd < 0)
    {
        perror("Project Sunrise Launcher: open");
        return 1;
    }


    off_t file_size =
        lseek(self_fd, 0, SEEK_END);

    if(file_size < FOOTER_SIZE)
    {
        fprintf(
            stderr,
            "Project Sunrise Launcher: invalid launcher file\n"
        );

        close(self_fd);
        return 1;
    }


    unsigned char footer[FOOTER_SIZE];

    if(lseek(
        self_fd,
        file_size - FOOTER_SIZE,
        SEEK_SET
    ) < 0)
    {
        perror("Project Sunrise Launcher: lseek");
        close(self_fd);
        return 1;
    }


    ssize_t footer_read =
        read(
            self_fd,
            footer,
            sizeof(footer)
        );

    if(footer_read != FOOTER_SIZE)
    {
        fprintf(
            stderr,
            "Project Sunrise Launcher: invalid launcher footer\n"
        );

        close(self_fd);
        return 1;
    }


    if(memcmp(
        footer,
        FOOTER_MAGIC,
        sizeof(FOOTER_MAGIC)
    ) != 0)
    {
        fprintf(
            stderr,
            "Project Sunrise Launcher: payload not found\n"
        );

        close(self_fd);
        return 1;
    }


    uint64_t payload_offset =
        read_le64(footer + 16);

    uint64_t payload_size =
        read_le64(footer + 24);


    if(
        payload_offset > (uint64_t)file_size ||
        payload_size > (uint64_t)file_size ||
        payload_offset + payload_size >
            (uint64_t)file_size - FOOTER_SIZE
    )
    {
        fprintf(
            stderr,
            "Project Sunrise Launcher: invalid payload\n"
        );

        close(self_fd);
        return 1;
    }


    const char *home = getenv("HOME");

    if(!home)
    {
        fprintf(
            stderr,
            "Project Sunrise Launcher: HOME is not set\n"
        );

        close(self_fd);
        return 1;
    }


    const char *xdg_data_home =
        getenv("XDG_DATA_HOME");

    char install_directory[PATH_MAX];

    if(xdg_data_home && xdg_data_home[0] != '\0')
    {
        snprintf(
            install_directory,
            sizeof(install_directory),
            "%s/project-sunrise-launcher",
            xdg_data_home
        );
    }
    else
    {
        snprintf(
            install_directory,
            sizeof(install_directory),
            "%s/project-sunrise-launcher",
            home
        );
    }


    if(mkdir_recursive(install_directory) != 0)
    {
        perror(
            "Project Sunrise Launcher: cannot create install directory"
        );

        close(self_fd);
        return 1;
    }


    if(extract_payload(
        self_fd,
        payload_offset,
        payload_size,
        install_directory
    ) != 0)
    {
        close(self_fd);
        return 1;
    }


    close(self_fd);


    char application_path[PATH_MAX];

    snprintf(
        application_path,
        sizeof(application_path),
        "%s/app/project-sunrise-launcher",
        install_directory
    );


    chmod(application_path, 0755);


    create_desktop_entry(
        self_path,
        install_directory
    );


    char **new_arguments =
        malloc(sizeof(char *) * ((size_t)argc + 1));

    if(!new_arguments)
    {
        fprintf(
            stderr,
            "Project Sunrise Launcher: out of memory\n"
        );

        return 1;
    }


    for(int i = 0; i < argc; i++)
    {
        new_arguments[i] = argv[i];
    }

    new_arguments[0] = application_path;
    new_arguments[argc] = NULL;


    execv(
        application_path,
        new_arguments
    );


    perror(
        "Project Sunrise Launcher: cannot start application"
    );

    free(new_arguments);

    return 1;
}