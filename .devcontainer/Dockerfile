#  Copyright (c) devcontainer-rs contributors.
#  Licensed under the MIT License.
ARG VARIANT="16-bullseye"
FROM node:${VARIANT}

RUN mkdir -p /workspaces && chown node:node /workspaces

ARG USERNAME=node
USER $USERNAME

# Save command line history
RUN echo "export HISTFILE=/home/$USERNAME/commandhistory/.bash_history" >> "/home/$USERNAME/.bashrc" \
	&& echo "export PROMPT_COMMAND='history -a'" >> "/home/$USERNAME/.bashrc" \
	&& mkdir -p /home/$USERNAME/commandhistory \
	&& touch /home/$USERNAME/commandhistory/.bash_history \
	&& chown -R $USERNAME /home/$USERNAME/commandhistory
